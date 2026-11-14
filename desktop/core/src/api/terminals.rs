//! A terminal of the host, streamed to a viewer over a WebSocket. The
//! person's window asks for a ticket, which opens one attach within 30
//! seconds, and the core relays the frames of the host's terminal channel
//! to the socket and back: a snapshot, then the output with its stream
//! offset, and the viewer's input, credit, size, control, focus and
//! display facts.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use pinrail_host::frame::{Frame, kind};
use pinrail_host::link::{Channel, HostError, HostState};
use serde_json::{Value, json};

use super::ApiState;
use super::guard::{Person, refuse};
use crate::runtime::LOCAL;

/// How long a ticket opens an attach.
const TICKET_FOR: Duration = Duration::from_secs(30);
/// The largest message a window may send.
const MAX_FROM_WINDOW: usize = 64 * 1024;
/// How long one message to the window may take to go out before the
/// window counts as too slow.
const SEND_WITHIN: Duration = Duration::from_secs(10);
/// The window protocol's version.
const VERSION: u64 = 1;

/// The close codes of the attach socket.
mod close {
    pub const DONE: u16 = 1000;
    pub const BAD_TICKET: u16 = 4001;
    pub const FORBIDDEN_ORIGIN: u16 = 4003;
    pub const UNKNOWN_TERMINAL: u16 = 4004;
    pub const TOO_SLOW: u16 = 4008;
    pub const HOST_OFFLINE: u16 = 4010;
    pub const REPLACED: u16 = 4011;
}

/// The binary message kinds.
const OUTPUT: u8 = 0x01;
const SNAPSHOT: u8 = 0x02;
const INPUT: u8 = 0x10;
const REPLY: u8 = 0x11;

/// The tickets not yet used: the terminal each opens, and when it expires.
#[derive(Clone, Default)]
pub struct Tickets(Arc<Mutex<HashMap<String, (String, Instant)>>>);

impl Tickets {
    fn issue(&self, terminal: &str) -> String {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).expect("the operating system's random numbers");
        let ticket: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let mut tickets = self.0.lock().unwrap_or_else(|e| e.into_inner());
        tickets.retain(|_, (_, until)| *until > Instant::now());
        tickets.insert(
            ticket.clone(),
            (terminal.to_string(), Instant::now() + TICKET_FOR),
        );
        ticket
    }

    /// Uses up a ticket: whether it was issued for `terminal` and has not
    /// expired.
    fn take(&self, ticket: &str, terminal: &str) -> bool {
        let mut tickets = self.0.lock().unwrap_or_else(|e| e.into_inner());
        tickets
            .remove(ticket)
            .is_some_and(|(issued, until)| issued == terminal && until > Instant::now())
    }
}

pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/terminals/{terminal}/tickets", post(ticket))
        .route("/api/v1/terminals/{terminal}/attach", get(attach))
}

/// A ticket that opens one attach to the terminal within 30 seconds.
/// The terminal is `<host>:<terminal id>`. Answers 201.
async fn ticket(
    State(state): State<ApiState>,
    _person: Person,
    Path(terminal): Path<String>,
) -> Response {
    if host_terminal(&terminal).is_none() {
        return refuse(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("terminal {terminal} not found"),
        );
    }
    let ticket = state.tickets.issue(&terminal);
    (
        StatusCode::CREATED,
        Json(json!({"ticket": ticket, "expires_in": TICKET_FOR.as_secs()})),
    )
        .into_response()
}

/// The host's own id of `<host>:<terminal id>`, on the one host so far.
fn host_terminal(terminal: &str) -> Option<&str> {
    terminal
        .strip_prefix(LOCAL)?
        .strip_prefix(':')
        .filter(|id| !id.is_empty())
}

/// Upgrades to the attach socket. A socket that may not attach is opened
/// and closed at once with the code that says why.
async fn attach(
    State(state): State<ApiState>,
    Path(terminal): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let refused = match headers
        .get(header::ORIGIN)
        .map(|o| o.to_str().unwrap_or(""))
    {
        Some(origin) if !super::shell_origins().iter().any(|s| s == origin) => {
            Some((close::FORBIDDEN_ORIGIN, "forbidden origin"))
        }
        _ => {
            let ticket = query.get("ticket").map(String::as_str).unwrap_or("");
            (!state.tickets.take(ticket, &terminal)).then_some((close::BAD_TICKET, "bad ticket"))
        }
    };
    let app = state.app.clone();
    upgrade
        .max_message_size(MAX_FROM_WINDOW)
        .on_upgrade(move |mut socket| async move {
            if let Some((code, reason)) = refused {
                close_with(&mut socket, code, reason).await;
                return;
            }
            relay(socket, &app, &terminal, &query).await;
        })
}

async fn close_with(socket: &mut WebSocket, code: u16, reason: &str) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code,
            reason: reason.into(),
        })))
        .await;
}

/// Attaches to the terminal through the host link, then relays until
/// either side ends.
async fn relay(
    mut socket: WebSocket,
    app: &crate::Pinrail,
    terminal: &str,
    query: &HashMap<String, String>,
) {
    let Some(id) = host_terminal(terminal) else {
        return close_with(&mut socket, close::UNKNOWN_TERMINAL, "unknown terminal").await;
    };
    let Some(link) = app
        .runtime()
        .host()
        .filter(|l| matches!(l.state(), HostState::Ready { .. }))
    else {
        return close_with(&mut socket, close::HOST_OFFLINE, "host offline").await;
    };
    let number = |key: &str| query.get(key).and_then(|v| v.parse::<u64>().ok());
    let mut params = json!({
        "terminal": id,
        "mode": "observe",
        "cols": number("cols").unwrap_or(80),
        "rows": number("rows").unwrap_or(24),
        "history": number("history").unwrap_or(5_000),
        "formats": query
            .get("formats")
            .map(|f| f.split(',').filter(|f| !f.is_empty()).collect::<Vec<_>>())
            .unwrap_or_else(|| vec!["vt"]),
    });
    if let Some(offset) = number("resume_from") {
        params["resume_from"] = json!(offset);
    }
    // the terminal's size, for the hello
    let listed = match link.call("terminal.list", json!({})).await {
        Ok(list) => list
            .as_array()
            .and_then(|l| l.iter().find(|t| t["terminal"] == id))
            .cloned(),
        Err(_) => return close_with(&mut socket, close::HOST_OFFLINE, "host offline").await,
    };
    let Some(listed) = listed.filter(|t| t.get("exit").is_none()) else {
        return close_with(&mut socket, close::UNKNOWN_TERMINAL, "unknown terminal").await;
    };
    let channel = match link.attach(params).await {
        Ok(channel) => channel,
        Err(HostError::Refused { .. }) => {
            return close_with(&mut socket, close::UNKNOWN_TERMINAL, "unknown terminal").await;
        }
        Err(_) => return close_with(&mut socket, close::HOST_OFFLINE, "host offline").await,
    };
    let hello = json!({
        "t": "hello",
        "v": VERSION,
        "terminal": terminal,
        "cols": listed["cols"],
        "rows": listed["rows"],
        "role": "observer",
        "features": [],
    });
    let mut channel = channel;
    if send(&mut socket, Message::Text(hello.to_string().into()))
        .await
        .is_err()
    {
        return;
    }
    let code = relay_frames(&mut socket, &mut channel).await;
    drop(channel);
    if let Some((code, reason)) = code {
        close_with(&mut socket, code, reason).await;
    }
}

/// Where the viewer's stream stands: inside a snapshot or not, and the
/// offset of the next output byte.
#[derive(Default)]
struct Stream {
    in_snapshot: bool,
    offset: u64,
}

/// Why sending to the window failed.
enum SendError {
    Gone,
    TooSlow,
}

async fn send(socket: &mut WebSocket, message: Message) -> Result<(), SendError> {
    match tokio::time::timeout(SEND_WITHIN, socket.send(message)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) => Err(SendError::Gone),
        Err(_) => Err(SendError::TooSlow),
    }
}

/// Relays until either side ends. Returns the code to close the socket
/// with, or `None` when the window went away.
async fn relay_frames(
    socket: &mut WebSocket,
    channel: &mut Channel,
) -> Option<(u16, &'static str)> {
    let mut stream = Stream::default();
    loop {
        tokio::select! {
            frame = channel.frames.recv() => {
                let Some(frame) = frame else {
                    return Some((close::HOST_OFFLINE, "host offline"));
                };
                if frame.kind == kind::CLOSE {
                    let reason: Value = serde_json::from_slice(&frame.payload).unwrap_or_default();
                    return Some(match reason["reason"].as_str() {
                        Some("exited") => (close::DONE, "exited"),
                        Some("replaced") => (close::REPLACED, "replaced"),
                        _ => (close::HOST_OFFLINE, "host offline"),
                    });
                }
                let Some(message) = to_window(&frame, &mut stream) else {
                    continue;
                };
                match send(socket, message).await {
                    Ok(()) => {}
                    Err(SendError::Gone) => return None,
                    Err(SendError::TooSlow) => return Some((close::TOO_SLOW, "too slow")),
                }
            }
            message = socket.recv() => {
                let message = match message {
                    Some(Ok(message)) => message,
                    _ => return None,
                };
                if matches!(message, Message::Close(_)) {
                    return None;
                }
                for (kind, payload) in from_window(&message) {
                    if channel.send(kind, payload).is_err() {
                        return Some((close::HOST_OFFLINE, "host offline"));
                    }
                }
            }
        }
    }
}

/// A frame of the host's terminal channel as the window reads it, or
/// `None` for one the window does not need.
fn to_window(frame: &Frame, stream: &mut Stream) -> Option<Message> {
    match frame.kind {
        kind::DATA if stream.in_snapshot => {
            let mut bytes = Vec::with_capacity(1 + frame.payload.len());
            bytes.push(SNAPSHOT);
            bytes.extend(&frame.payload);
            Some(Message::Binary(bytes.into()))
        }
        kind::DATA => {
            let mut bytes = Vec::with_capacity(9 + frame.payload.len());
            bytes.push(OUTPUT);
            bytes.extend(stream.offset.to_le_bytes());
            bytes.extend(&frame.payload);
            stream.offset += frame.payload.len() as u64;
            Some(Message::Binary(bytes.into()))
        }
        kind::META => {
            let mut value: Value = serde_json::from_slice(&frame.payload).ok()?;
            let object = value.as_object_mut()?;
            let kind = object.remove("type")?;
            match kind.as_str()? {
                "snapshot_begin" => {
                    stream.in_snapshot = true;
                    // the offset of the cut: output after the snapshot
                    // starts there
                    let cut = object.remove("offset").unwrap_or(json!(0));
                    stream.offset = cut.as_u64().unwrap_or(0);
                    object.insert("cut".into(), cut);
                }
                "snapshot_end" => stream.in_snapshot = false,
                "resume" => stream.offset = object.get("offset")?.as_u64()?,
                _ => {}
            }
            object.insert("t".into(), kind);
            Some(Message::Text(value.to_string().into()))
        }
        _ => None,
    }
}

/// The frames a window's message becomes on the host's terminal channel.
/// A message the window protocol does not have becomes none.
fn from_window(message: &Message) -> Vec<(u8, Vec<u8>)> {
    match message {
        Message::Binary(bytes) => match bytes.split_first() {
            Some((&INPUT, input)) => vec![(kind::DATA, input.to_vec())],
            Some((&REPLY, reply)) => vec![(kind::REPLY, reply.to_vec())],
            _ => vec![],
        },
        Message::Text(text) => {
            let Ok(value) = serde_json::from_str::<Value>(text) else {
                return vec![];
            };
            let size = |key: &str| {
                value[key]
                    .as_u64()
                    .and_then(|n| u16::try_from(n).ok())
                    .filter(|n| *n > 0)
            };
            match value["t"].as_str() {
                Some("credit") => value["bytes"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .map(|n| vec![(kind::CREDIT, n.to_le_bytes().to_vec())])
                    .unwrap_or_default(),
                Some("resize") => match (size("cols"), size("rows")) {
                    (Some(cols), Some(rows)) => {
                        let mut payload = cols.to_le_bytes().to_vec();
                        payload.extend(rows.to_le_bytes());
                        vec![(kind::RESIZE, payload)]
                    }
                    _ => vec![],
                },
                Some("control") => {
                    let take = if value["take"] == true {
                        "take"
                    } else {
                        "release"
                    };
                    vec![(kind::CONTROL, json!(take).to_string().into_bytes())]
                }
                Some("facts") => {
                    let facts = json!({
                        "foreground": value["fg"],
                        "background": value["bg"],
                        "cursor": value["cursor"],
                        "color_scheme": value["color_scheme"],
                    });
                    vec![(kind::FACTS, facts.to_string().into_bytes())]
                }
                Some("focus") => vec![(kind::FOCUS, vec![u8::from(value["in"] == true)])],
                _ => vec![],
            }
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(value: Value) -> Frame {
        Frame {
            channel: 1,
            kind: kind::META,
            payload: value.to_string().into_bytes(),
        }
    }

    fn data(bytes: &[u8]) -> Frame {
        Frame {
            channel: 1,
            kind: kind::DATA,
            payload: bytes.to_vec(),
        }
    }

    fn binary(message: Option<Message>) -> Vec<u8> {
        match message {
            Some(Message::Binary(bytes)) => bytes.to_vec(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn output_after_a_snapshot_carries_its_offset_from_the_cut() {
        let mut stream = Stream::default();
        let begin = to_window(
            &meta(
                json!({"type": "snapshot_begin", "format": "vt", "cols": 80, "rows": 24, "offset": 100}),
            ),
            &mut stream,
        );
        let Some(Message::Text(begin)) = begin else {
            panic!()
        };
        let begin: Value = serde_json::from_str(&begin).unwrap();
        assert_eq!(begin["t"], "snapshot_begin");
        assert_eq!(begin["cut"], 100);
        assert!(begin.get("offset").is_none());
        assert_eq!(
            binary(to_window(&data(b"screen"), &mut stream)),
            b"\x02screen"
        );
        to_window(&meta(json!({"type": "snapshot_end"})), &mut stream);
        let mut first = vec![OUTPUT];
        first.extend(100u64.to_le_bytes());
        first.extend(b"ab");
        assert_eq!(binary(to_window(&data(b"ab"), &mut stream)), first);
        let mut second = vec![OUTPUT];
        second.extend(102u64.to_le_bytes());
        second.extend(b"c");
        assert_eq!(binary(to_window(&data(b"c"), &mut stream)), second);
    }

    #[test]
    fn a_resumed_stream_continues_from_the_offset_the_viewer_read_to() {
        let mut stream = Stream::default();
        to_window(&meta(json!({"type": "resume", "offset": 7})), &mut stream);
        assert_eq!(
            &binary(to_window(&data(b"x"), &mut stream))[1..9],
            7u64.to_le_bytes()
        );
    }

    #[test]
    fn the_windows_messages_become_the_hosts_frames() {
        let text = |value: Value| Message::Text(value.to_string().into());
        assert_eq!(
            from_window(&Message::Binary(b"\x10ls".to_vec().into())),
            [(kind::DATA, b"ls".to_vec())]
        );
        assert_eq!(
            from_window(&Message::Binary(b"\x11\x1b[1;1R".to_vec().into())),
            [(kind::REPLY, b"\x1b[1;1R".to_vec())]
        );
        assert_eq!(
            from_window(&text(json!({"t": "credit", "bytes": 4096}))),
            [(kind::CREDIT, 4096u32.to_le_bytes().to_vec())]
        );
        assert_eq!(
            from_window(&text(json!({"t": "resize", "cols": 132, "rows": 40}))),
            [(kind::RESIZE, vec![132, 0, 40, 0])]
        );
        assert_eq!(
            from_window(&text(json!({"t": "control", "take": true}))),
            [(kind::CONTROL, b"\"take\"".to_vec())]
        );
        assert_eq!(
            from_window(&text(json!({"t": "focus", "in": true}))),
            [(kind::FOCUS, vec![1])]
        );
        let facts = from_window(&text(
            json!({"t": "facts", "fg": "#ffffff", "bg": "#000000"}),
        ));
        let sent: Value = serde_json::from_slice(&facts[0].1).unwrap();
        assert_eq!(sent["foreground"], "#ffffff");
        assert_eq!(sent["background"], "#000000");
        // what the protocol does not have is dropped
        assert!(from_window(&Message::Binary(b"\x7fx".to_vec().into())).is_empty());
        assert!(from_window(&text(json!({"t": "resize", "cols": 0, "rows": 5}))).is_empty());
    }
}
