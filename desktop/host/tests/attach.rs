//! Terminal channels: a viewer attaches, gets a snapshot and then the
//! output, within the credit it returns, takes control with its first key,
//! and resumes after a reconnect with only what it missed.
#![cfg(unix)]

mod common;

use std::collections::{BTreeMap, VecDeque};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use pinrail_host::frame::{self, Frame, kind};
use pinrail_screen::{Options, Screen, TextRange};
use serde_json::{Value, json};

use common::{Folders, folders, serve};

/// A controller connection, with the frames of each channel kept apart.
struct Connection {
    stream: UnixStream,
    frames: mpsc::Receiver<Frame>,
    channels: BTreeMap<u32, VecDeque<Frame>>,
    next_id: u64,
}

impl Connection {
    fn open(host: &common::Host) -> Self {
        let mut stream = UnixStream::connect(&host.registration.socket).unwrap();
        let hello = json!({"jsonrpc": "2.0", "id": 0, "method": "hello",
                           "params": {"secret": host.registration.secret}});
        frame::write_blocking(&mut stream, &Frame::json(&hello)).unwrap();
        let mut reader = stream.try_clone().unwrap();
        let (tx, frames) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(Some(frame)) = frame::read_blocking(&mut reader) {
                if tx.send(frame).is_err() {
                    break;
                }
            }
        });
        let mut connection = Self {
            stream,
            frames,
            channels: BTreeMap::new(),
            next_id: 1,
        };
        connection.reply(0);
        connection
    }

    /// Takes frames until `stop` says so, keeping terminal frames per
    /// channel.
    fn pump(&mut self, within: Duration, mut stop: impl FnMut(&Frame) -> bool) -> Option<Frame> {
        let deadline = Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let frame = self.frames.recv_timeout(left).ok()?;
            if stop(&frame) {
                return Some(frame);
            }
            if frame.channel != 0 {
                self.channels
                    .entry(frame.channel)
                    .or_default()
                    .push_back(frame);
            }
        }
    }

    fn reply(&mut self, id: u64) -> Value {
        let frame = self
            .pump(Duration::from_secs(10), |f| {
                f.channel == 0
                    && serde_json::from_slice::<Value>(&f.payload).is_ok_and(|m| m["id"] == id)
            })
            .expect("a reply");
        serde_json::from_slice(&frame.payload).unwrap()
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        frame::write_blocking(&mut self.stream, &Frame::json(&message)).unwrap();
        self.reply(id)
    }

    fn send(&mut self, channel: u32, kind: u8, payload: Vec<u8>) {
        frame::write_blocking(
            &mut self.stream,
            &Frame {
                channel,
                kind,
                payload,
            },
        )
        .unwrap();
    }

    /// The next frame on a channel.
    fn next(&mut self, channel: u32) -> Option<Frame> {
        if let Some(frame) = self
            .channels
            .get_mut(&channel)
            .and_then(VecDeque::pop_front)
        {
            return Some(frame);
        }
        self.pump(Duration::from_secs(10), |f| f.channel == channel)
    }

    /// Frames on a channel until a META frame of `kind_name`, which is
    /// returned with the bytes of the DATA frames before it.
    fn until_meta(&mut self, channel: u32, kind_name: &str) -> (Value, Vec<u8>) {
        let mut bytes = Vec::new();
        loop {
            let frame = self
                .next(channel)
                .unwrap_or_else(|| panic!("no {kind_name}"));
            match frame.kind {
                kind::DATA => bytes.extend(frame.payload),
                kind::META => {
                    let meta: Value = serde_json::from_slice(&frame.payload).unwrap();
                    if meta["type"] == kind_name {
                        return (meta, bytes);
                    }
                }
                _ => {}
            }
        }
    }

    /// Output on a channel until it contains `text`.
    fn output_until(&mut self, channel: u32, text: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        while !bytes.windows(text.len()).any(|w| w == text) {
            let frame = self.next(channel).expect("more output");
            if frame.kind == kind::DATA {
                bytes.extend(frame.payload);
            }
        }
        bytes
    }
}

fn spawn(connection: &mut Connection, folders: &Folders, script: &str) -> String {
    let reply = connection.call(
        "terminal.spawn",
        json!({"argv": ["sh", "-c", script], "cwd": folders.runtime, "cols": 60, "rows": 12,
               "env_base": "host"}),
    );
    reply["result"]["terminal"].as_str().unwrap().to_string()
}

fn attach(connection: &mut Connection, terminal: &str, extra: Value) -> (u32, String) {
    let mut params = json!({"terminal": terminal, "mode": "observe", "cols": 60, "rows": 12,
                            "history": 100, "formats": ["vt"]});
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    let reply = connection.call("terminal.attach", params);
    assert!(reply["result"].is_object(), "{reply}");
    (
        reply["result"]["channel"].as_u64().unwrap() as u32,
        reply["result"]["format"].as_str().unwrap().to_string(),
    )
}

fn screen(bytes: &[u8], cols: u16, rows: u16) -> String {
    let mut screen = pinrail_screen::ghostty::Terminal::new(&Options {
        cols,
        rows,
        scrollback_lines: 1_000,
    })
    .unwrap();
    screen.write(bytes);
    screen.text(TextRange::Screen)
}

fn wait_for_text(connection: &mut Connection, terminal: &str, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let reply = connection.call("terminal.text", json!({"terminal": terminal, "lines": 0}));
        if reply["result"]["text"]
            .as_str()
            .unwrap_or("")
            .contains(text)
        {
            return;
        }
        assert!(Instant::now() < deadline, "no {text}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_viewer_gets_the_screen_then_the_output_without_answered_queries() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut connection = Connection::open(&host);
    let terminal = spawn(
        &mut connection,
        &folders,
        "printf 'first screen\\r\\n'; read x; printf 'then \\033[c%s\\r\\n' \"$x\"; sleep 30",
    );
    wait_for_text(&mut connection, &terminal, "first screen");
    let (channel, format) = attach(&mut connection, &terminal, json!({}));
    assert_eq!(format, "vt");
    // the reply came before any frame of the channel, which starts with the
    // snapshot
    let (begin, before) = connection.until_meta(channel, "snapshot_begin");
    assert!(before.is_empty());
    assert_eq!(
        (begin["cols"].clone(), begin["rows"].clone()),
        (json!(60), json!(12))
    );
    let (_, paint) = connection.until_meta(channel, "snapshot_end");
    assert!(screen(&paint, 60, 12).contains("first screen"));
    // the first key makes this viewer the controller
    connection.send(channel, kind::DATA, b"typed\r".to_vec());
    let (role, _) = connection.until_meta(channel, "role");
    assert_eq!(role["role"], "controller");
    let output = connection.output_until(channel, b"typed");
    // the device attributes query was answered by the host, not passed on
    assert!(
        !output.windows(3).any(|w| w == b"\x1b[c"),
        "{:?}",
        String::from_utf8_lossy(&output)
    );
    connection.call("terminal.stop", json!({"terminal": terminal}));
    let (exit, _) = connection.until_meta(channel, "exit");
    assert_eq!(exit["reason"], "stopped");
    let close = connection.next(channel).unwrap();
    assert_eq!(close.kind, kind::CLOSE);
}

#[test]
fn a_viewer_of_the_same_library_build_gets_the_binary_snapshot() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut connection = Connection::open(&host);
    let terminal = spawn(
        &mut connection,
        &folders,
        "printf 'binary please\\r\\n'; sleep 30",
    );
    wait_for_text(&mut connection, &terminal, "binary please");
    let binary = format!("ghostty-bin;vt-{}", pinrail_screen::ghostty::version());
    let (channel, format) = attach(
        &mut connection,
        &terminal,
        json!({"formats": [binary, "vt"]}),
    );
    assert_eq!(format, binary);
    let (begin, _) = connection.until_meta(channel, "snapshot_begin");
    assert_eq!(begin["format"], binary.as_str());
    let (_, bytes) = connection.until_meta(channel, "snapshot_end");
    let restored = pinrail_screen::ghostty::Terminal::restore(&bytes, 1_000).unwrap();
    assert!(restored.text(TextRange::Screen).contains("binary please"));
    // a format the host cannot produce is refused
    let reply = connection.call(
        "terminal.attach",
        json!({"terminal": terminal, "cols": 60, "rows": 12, "formats": ["sixel-movie"]}),
    );
    assert_eq!(reply["error"]["data"]["code"], "unsupported");
}

#[test]
fn the_controller_s_size_is_the_terminal_s_and_observers_get_a_fresh_snapshot() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut connection = Connection::open(&host);
    connection.call("events.subscribe", json!({}));
    let terminal = spawn(
        &mut connection,
        &folders,
        "trap 'stty size' WINCH; echo ready; while :; do sleep 0.05; done",
    );
    wait_for_text(&mut connection, &terminal, "ready");
    let (observer, _) = attach(&mut connection, &terminal, json!({}));
    connection.until_meta(observer, "snapshot_end");
    let (controller, _) = attach(&mut connection, &terminal, json!({"cols": 80, "rows": 20}));
    connection.until_meta(controller, "snapshot_end");
    // taking control resizes the terminal to the controller's size
    connection.send(controller, kind::CONTROL, b"\"take\"".to_vec());
    let (role, _) = connection.until_meta(controller, "role");
    assert_eq!(role["role"], "controller");
    let (size, _) = connection.until_meta(controller, "size");
    assert_eq!(
        (size["cols"].clone(), size["rows"].clone()),
        (json!(80), json!(20))
    );
    let output = connection.output_until(controller, b"20 80");
    assert!(!output.is_empty());
    // the observer is told the size and gets a snapshot laid out for it
    let (size, _) = connection.until_meta(observer, "size");
    assert_eq!(size["cols"], 80);
    let (begin, _) = connection.until_meta(observer, "snapshot_begin");
    assert_eq!(begin["cols"], 80);
    // an observer's resize changes nothing
    let mut resize = 100u16.to_le_bytes().to_vec();
    resize.extend(30u16.to_le_bytes());
    connection.send(observer, kind::RESIZE, resize);
    let list = connection.call("terminal.list", json!({}));
    assert_eq!(list["result"][0]["cols"], 80);
    connection.call("terminal.stop", json!({"terminal": terminal}));
}

#[test]
fn a_viewer_that_returns_no_credit_is_resynchronized_when_it_does() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut connection = Connection::open(&host);
    let terminal = spawn(
        &mut connection,
        &folders,
        "echo ready; read x; awk 'BEGIN { for (i = 0; i < 300000; i++) print \"line \" i \" of a lot of output\" }'; echo done; sleep 30",
    );
    wait_for_text(&mut connection, &terminal, "ready");
    let (channel, _) = attach(&mut connection, &terminal, json!({}));
    connection.until_meta(channel, "snapshot_end");
    connection.send(channel, kind::DATA, b"\r".to_vec());
    wait_for_text(&mut connection, &terminal, "done");
    // only the first 256 KiB went out
    let mut received = 0;
    while let Some(frame) = connection.pump(Duration::from_millis(300), |f| f.channel == channel) {
        if frame.kind == kind::DATA {
            received += frame.payload.len();
        }
    }
    assert!(received <= 256 * 1024 + 64 * 1024, "{received}");
    // returning credit brings a resync and a snapshot of the screen now
    connection.send(channel, kind::CREDIT, (1u32 << 20).to_le_bytes().to_vec());
    connection.until_meta(channel, "resync");
    connection.until_meta(channel, "snapshot_begin");
    let (_, paint) = connection.until_meta(channel, "snapshot_end");
    assert!(screen(&paint, 60, 12).contains("done"));
    connection.call("terminal.stop", json!({"terminal": terminal}));
}

#[test]
fn a_viewer_that_reconnects_gets_only_what_it_missed() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut connection = Connection::open(&host);
    let terminal = spawn(
        &mut connection,
        &folders,
        "while read x; do echo \"got $x\"; done",
    );
    let (channel, _) = attach(&mut connection, &terminal, json!({}));
    let (begin, _) = connection.until_meta(channel, "snapshot_begin");
    connection.until_meta(channel, "snapshot_end");
    let mut offset = begin["offset"].as_u64().unwrap();
    connection.send(channel, kind::DATA, b"one\r".to_vec());
    offset += connection.output_until(channel, b"got one").len() as u64;
    connection.send(channel, kind::CLOSE, b"{}".to_vec());
    // more output while no viewer is attached
    let (other, _) = attach(&mut connection, &terminal, json!({}));
    connection.until_meta(other, "snapshot_end");
    connection.send(other, kind::DATA, b"two\r".to_vec());
    connection.output_until(other, b"got two");
    let (resumed, format) = attach(&mut connection, &terminal, json!({"resume_from": offset}));
    assert_eq!(format, "resume");
    connection.until_meta(resumed, "resume");
    let missed = connection.output_until(resumed, b"got two");
    assert!(
        !missed.windows(7).any(|w| w == b"got one"),
        "{:?}",
        String::from_utf8_lossy(&missed)
    );
    // an offset the ring no longer holds gets a snapshot
    let (_, format) = attach(
        &mut connection,
        &terminal,
        json!({"resume_from": offset + 1_000_000_000}),
    );
    assert_eq!(format, "vt");
    connection.call("terminal.stop", json!({"terminal": terminal}));
}

#[test]
fn only_the_controller_s_replies_and_focus_reach_the_program() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut connection = Connection::open(&host);
    let terminal = spawn(
        &mut connection,
        &folders,
        "stty raw -echo; printf '\\033[?1004h'; echo ready; \
         r=$(dd bs=1 count=9 2>/dev/null | od -An -c | tr -d ' \\n'); printf '\\r\\ngot:%s\\r\\n' \"$r\"; sleep 30",
    );
    wait_for_text(&mut connection, &terminal, "ready");
    let (observer, _) = attach(&mut connection, &terminal, json!({}));
    let (controller, _) = attach(&mut connection, &terminal, json!({}));
    connection.until_meta(observer, "snapshot_end");
    connection.until_meta(controller, "snapshot_end");
    connection.send(controller, kind::CONTROL, b"\"take\"".to_vec());
    connection.until_meta(controller, "role");
    // from an observer, neither counts
    connection.send(observer, kind::REPLY, b"XX".to_vec());
    connection.send(observer, kind::FOCUS, vec![1]);
    std::thread::sleep(Duration::from_millis(200));
    // the controller's focus, then its emulator's reply
    connection.send(controller, kind::FOCUS, vec![1]);
    connection.send(controller, kind::REPLY, b"ok!".to_vec());
    let output = connection.output_until(controller, b"got:");
    let text = String::from_utf8_lossy(&output);
    // focus out when the program asked, as no viewer controlled it; then
    // the controller's focus in and its reply, and nothing of the observer's
    assert!(
        screen(&output, 60, 12).contains("got:033[O033[Iok!"),
        "{text}"
    );
    connection.call("terminal.stop", json!({"terminal": terminal}));
}
