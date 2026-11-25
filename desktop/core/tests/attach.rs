//! A window attaches to a terminal of the host through the core: with a
//! ticket of the person's, it gets a snapshot, then the output from the
//! cut on, types to take control, and hears of a resize and the exit.
#![cfg(unix)]

mod common;

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use pinrail_core::runtime;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use common::{App, call};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A core linked to a host, served on a port of its own.
struct Served {
    app: App,
    port: u16,
    _hosts: tempfile::TempDir,
}

async fn served() -> Served {
    let hosts = tempfile::tempdir().unwrap();
    let registration = common::host(hosts.path());
    let app = common::app_with(|config| {
        config.host_socket = Some((registration.socket.clone(), registration.secret.clone()));
    });
    runtime::watch(app.state.clone());
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while app.state.runtime().status()["state"] != "ready" {
        assert!(
            std::time::Instant::now() < deadline,
            "the host did not connect"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = app.router.clone();
    tokio::spawn(async move { axum::serve(listener, router).await });
    Served {
        app,
        port,
        _hosts: hosts,
    }
}

impl Served {
    /// Starts `script` in a terminal of the host, through the core's link,
    /// and returns its name as the window gives it.
    async fn spawn(&self, script: &str) -> String {
        let spawned = self
            .app
            .state
            .runtime()
            .host()
            .unwrap()
            .call(
                "terminal.spawn",
                json!({"argv": ["sh", "-c", script], "cwd": "/", "cols": 80, "rows": 24,
                       "env": {"PATH": "/usr/bin:/bin"}, "env_base": "host"}),
            )
            .await
            .unwrap();
        format!("local:{}", spawned["terminal"].as_str().unwrap())
    }

    async fn ticket(&self, terminal: &str) -> String {
        let (status, body) = call(
            &self.app,
            "POST",
            &format!("/api/v1/terminals/{terminal}/tickets"),
            None,
        )
        .await;
        assert_eq!(status, 201, "{body}");
        body["ticket"].as_str().unwrap().to_string()
    }

    async fn connect(&self, terminal: &str, query: &str, origin: Option<&str>) -> Socket {
        let url = format!(
            "ws://127.0.0.1:{}/api/v1/terminals/{terminal}/attach?{query}",
            self.port
        );
        let mut request = url.into_client_request().unwrap();
        if let Some(origin) = origin {
            request
                .headers_mut()
                .insert("origin", origin.parse().unwrap());
        }
        tokio_tungstenite::connect_async(request).await.unwrap().0
    }

    async fn attach(&self, terminal: &str) -> Socket {
        let ticket = self.ticket(terminal).await;
        self.connect(
            terminal,
            &format!("ticket={ticket}&cols=80&rows=24&history=100"),
            Some("tauri://localhost"),
        )
        .await
    }
}

/// What the window reads.
#[derive(Debug)]
enum Read {
    Text(Value),
    Output(u64, Vec<u8>),
    Snapshot(Vec<u8>),
    Closed(Option<u16>),
}

async fn read(socket: &mut Socket) -> Read {
    let message = tokio::time::timeout(Duration::from_secs(10), socket.next())
        .await
        .expect("nothing came within 10 seconds");
    match message {
        Some(Ok(Message::Text(text))) => Read::Text(serde_json::from_str(&text).unwrap()),
        Some(Ok(Message::Binary(bytes))) => match bytes[0] {
            0x01 => Read::Output(
                u64::from_le_bytes(bytes[1..9].try_into().unwrap()),
                bytes[9..].to_vec(),
            ),
            0x02 => Read::Snapshot(bytes[1..].to_vec()),
            other => panic!("binary message {other:#x}"),
        },
        Some(Ok(Message::Close(frame))) => Read::Closed(frame.map(|f| u16::from(f.code))),
        None | Some(Err(_)) => Read::Closed(None),
        Some(Ok(other)) => panic!("{other:?}"),
    }
}

/// Reads until a text message of type `t`, and returns it.
async fn text(socket: &mut Socket, t: &str) -> Value {
    loop {
        match read(socket).await {
            Read::Text(value) if value["t"] == t => return value,
            Read::Closed(code) => panic!("closed with {code:?} before {t}"),
            _ => {}
        }
    }
}

/// The snapshot after a `snapshot_begin`, and its cut.
async fn snapshot(socket: &mut Socket) -> (u64, String) {
    let begin = text(socket, "snapshot_begin").await;
    let mut bytes = Vec::new();
    loop {
        match read(socket).await {
            Read::Snapshot(chunk) => bytes.extend(chunk),
            Read::Text(value) if value["t"] == "snapshot_end" => break,
            other => panic!("{other:?} inside a snapshot"),
        }
    }
    (
        begin["cut"].as_u64().unwrap(),
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

/// Reads output until it holds `wanted`, checking that each piece starts
/// where the last ended.
async fn output_until(socket: &mut Socket, mut offset: u64, wanted: &str) -> u64 {
    let mut seen = Vec::new();
    loop {
        match read(socket).await {
            Read::Output(at, bytes) => {
                assert_eq!(at, offset, "output out of order");
                offset += bytes.len() as u64;
                credit(socket, bytes.len()).await;
                seen.extend(bytes);
                // only the end of what came is searched
                if seen.len() > 64 * 1024 {
                    seen.drain(..seen.len() - 1024);
                }
                if String::from_utf8_lossy(&seen).contains(wanted) {
                    return offset;
                }
            }
            Read::Closed(code) => panic!("closed with {code:?} before {wanted:?}"),
            _ => {}
        }
    }
}

async fn credit(socket: &mut Socket, bytes: usize) {
    socket
        .send(Message::Text(
            json!({"t": "credit", "bytes": bytes}).to_string().into(),
        ))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_gets_a_snapshot_then_the_output_from_its_cut() {
    let served = served().await;
    let terminal = served
        .spawn("printf 'ready\\n'; read line; printf 'got:%s\\n' \"$line\"; sleep 600")
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut socket = served.attach(&terminal).await;

    let hello = text(&mut socket, "hello").await;
    assert_eq!(hello["v"], 1);
    assert_eq!(hello["role"], "observer");
    assert_eq!(hello["terminal"], terminal.as_str());
    assert_eq!(
        (hello["cols"].as_u64(), hello["rows"].as_u64()),
        (Some(80), Some(24))
    );
    let (cut, screen) = snapshot(&mut socket).await;
    assert!(screen.contains("ready"), "{screen:?}");

    // typing takes control, and the output follows from the cut
    socket
        .send(Message::Binary(b"\x10hello\r".to_vec().into()))
        .await
        .unwrap();
    let role = text(&mut socket, "role").await;
    assert_eq!(role["role"], "controller");
    output_until(&mut socket, cut, "got:hello").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_observer_hears_of_the_controllers_resize_and_gets_a_new_snapshot() {
    let served = served().await;
    let terminal = served.spawn("printf 'ready\\n'; sleep 600").await;
    let mut controller = served.attach(&terminal).await;
    let mut observer = served.attach(&terminal).await;
    snapshot(&mut controller).await;
    snapshot(&mut observer).await;

    controller
        .send(Message::Text(
            json!({"t": "control", "take": true}).to_string().into(),
        ))
        .await
        .unwrap();
    assert_eq!(text(&mut controller, "role").await["role"], "controller");
    // an observer's resize changes nothing
    observer
        .send(Message::Text(
            json!({"t": "resize", "cols": 50, "rows": 10})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    controller
        .send(Message::Text(
            json!({"t": "resize", "cols": 100, "rows": 30})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let size = text(&mut observer, "size").await;
    assert_eq!(
        (size["cols"].as_u64(), size["rows"].as_u64()),
        (Some(100), Some(30))
    );
    let begin = text(&mut observer, "snapshot_begin").await;
    assert_eq!(
        (begin["cols"].as_u64(), begin["rows"].as_u64()),
        (Some(100), Some(30))
    );
    let size = text(&mut controller, "size").await;
    assert_eq!(size["cols"], 100);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_that_returns_no_credit_is_resynced_without_slowing_another() {
    let served = served().await;
    let terminal = served
        .spawn("read go; head -c 6000000 /dev/zero | tr '\\0' x; printf 'END\\n'; sleep 600")
        .await;
    let mut stalled = served.attach(&terminal).await;
    let mut reading = served.attach(&terminal).await;
    snapshot(&mut stalled).await;
    let (cut, _) = snapshot(&mut reading).await;

    reading
        .send(Message::Binary(b"\x10go\r".to_vec().into()))
        .await
        .unwrap();
    // the window that reads gets every byte, while the other returns none
    output_until(&mut reading, cut, "END").await;

    // credit from the stalled window brings a resync and a fresh snapshot
    credit(&mut stalled, 1024 * 1024).await;
    text(&mut stalled, "resync").await;
    let (_, screen) = snapshot(&mut stalled).await;
    assert!(screen.contains("END"), "{screen:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_socket_closes_with_the_exit_of_the_terminal() {
    let served = served().await;
    let terminal = served.spawn("read line; exit 3").await;
    let mut socket = served.attach(&terminal).await;
    snapshot(&mut socket).await;
    socket
        .send(Message::Binary(b"\x10bye\r".to_vec().into()))
        .await
        .unwrap();
    let exit = text(&mut socket, "exit").await;
    assert_eq!(exit["code"], 3);
    loop {
        if let Read::Closed(code) = read(&mut socket).await {
            assert_eq!(code, Some(u16::from(CloseCode::Normal)));
            break;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_attach_needs_a_fresh_ticket_from_the_window() {
    let served = served().await;
    let terminal = served.spawn("sleep 600").await;
    let closed = |code: u16| move |read: Read| matches!(read, Read::Closed(Some(c)) if c == code);

    // only the person gets a ticket
    let (status, _) = common::call_as(
        &served.app,
        "POST",
        &format!("/api/v1/terminals/{terminal}/tickets"),
        None,
        &[],
    )
    .await;
    assert_eq!(status, 401);

    let mut socket = served
        .connect(&terminal, "ticket=nope", Some("tauri://localhost"))
        .await;
    assert!(closed(4001)(read(&mut socket).await));

    // a ticket opens one attach, of its own terminal
    let ticket = served.ticket(&terminal).await;
    let mut socket = served
        .connect("local:t_other", &format!("ticket={ticket}"), None)
        .await;
    assert!(closed(4001)(read(&mut socket).await));
    let ticket = served.ticket(&terminal).await;
    let mut socket = served
        .connect(&terminal, &format!("ticket={ticket}"), None)
        .await;
    assert_eq!(text(&mut socket, "hello").await["t"], "hello");
    let mut again = served
        .connect(&terminal, &format!("ticket={ticket}"), None)
        .await;
    assert!(closed(4001)(read(&mut again).await));

    // a page of another origin is refused
    let ticket = served.ticket(&terminal).await;
    let mut socket = served
        .connect(
            &terminal,
            &format!("ticket={ticket}"),
            Some("https://evil.example"),
        )
        .await;
    assert!(closed(4003)(read(&mut socket).await));

    // a terminal the host does not have
    let ticket = served.ticket("local:t_none").await;
    let mut socket = served
        .connect("local:t_none", &format!("ticket={ticket}"), None)
        .await;
    assert!(closed(4004)(read(&mut socket).await));
}
