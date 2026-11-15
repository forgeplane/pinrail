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

#[test]
fn a_viewer_that_attaches_inside_a_long_sequence_gets_its_snapshot_at_its_end() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut connection = Connection::open(&host);
    // an image sequence longer than a snapshot can replay, left open
    let terminal = spawn(
        &mut connection,
        &folders,
        "echo ready; printf '\\033_Gf=100;'; head -c 100000 /dev/zero | tr '\\0' A; read x; printf '\\033\\\\after\\n'; sleep 30",
    );
    wait_for_text(&mut connection, &terminal, "ready");
    std::thread::sleep(Duration::from_millis(500));
    let (channel, format) = attach(&mut connection, &terminal, json!({}));
    assert_eq!(format, "vt");
    // nothing while the sequence is open
    assert!(
        connection
            .pump(Duration::from_millis(500), |f| f.channel == channel)
            .is_none()
    );

    connection.send(channel, kind::DATA, b"\r".to_vec());
    let (begin, _) = connection.until_meta(channel, "snapshot_begin");
    assert!(begin["offset"].as_u64().unwrap() > 100_000, "{begin}");
    let (_, mut paint) = connection.until_meta(channel, "snapshot_end");
    // the end of the sequence and what follows it may be in the snapshot
    // or after it
    while let Some(frame) = connection.pump(Duration::from_millis(300), |f| f.channel == channel) {
        if frame.kind == kind::DATA {
            paint.extend(frame.payload);
        }
    }
    let text = screen(&paint, 60, 12);
    assert!(text.contains("ready") && text.contains("after"), "{text}");
    assert!(!text.contains("AAAA"), "{text}");
    connection.call("terminal.stop", json!({"terminal": terminal}));
}

#[test]
fn a_viewer_that_attaches_inside_a_query_gets_a_well_formed_stream() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut connection = Connection::open(&host);
    // a cursor-position query cut in two, which the host's screen answers
    let terminal = spawn(
        &mut connection,
        &folders,
        "echo ready; printf '\\033[6'; sleep 1; printf 'n'; echo after; sleep 30",
    );
    wait_for_text(&mut connection, &terminal, "ready");
    std::thread::sleep(Duration::from_millis(300));
    let (channel, _) = attach(&mut connection, &terminal, json!({}));
    let (_, mut stream) = connection.until_meta(channel, "snapshot_end");
    stream.extend(connection.output_until(channel, b"after"));
    // the viewer's emulator ends up outside any sequence: the text after
    // the query is whole
    let text = screen(&stream, 60, 12);
    assert!(text.contains("after"), "{text:?}");
    connection.call("terminal.stop", json!({"terminal": terminal}));
}

#[test]
fn a_viewer_that_attaches_inside_a_sequence_gets_it_once() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut connection = Connection::open(&host);
    // a colour sequence, which is no query, cut in two
    let terminal = spawn(
        &mut connection,
        &folders,
        "echo ready; printf '\\033[3'; sleep 1; printf '1mred\\033[0m\\n'; sleep 30",
    );
    wait_for_text(&mut connection, &terminal, "ready");
    std::thread::sleep(Duration::from_millis(300));
    let (channel, _) = attach(&mut connection, &terminal, json!({}));
    let (_, mut stream) = connection.until_meta(channel, "snapshot_end");
    stream.extend(connection.output_until(channel, b"red"));
    let text = screen(&stream, 60, 12);
    assert!(text.contains("red"), "{text:?}");
    assert!(!text.contains("1m"), "{text:?}");
    connection.call("terminal.stop", json!({"terminal": terminal}));
}

#[test]
fn the_colours_a_viewer_sent_as_an_observer_apply_once_it_takes_control() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut connection = Connection::open(&host);
    // asks for the background colour, and prints the answer without its
    // escape bytes
    let terminal = spawn(
        &mut connection,
        &folders,
        "stty -icanon -echo min 1 time 0; echo ready; sleep 2; printf '\\033]11;?\\033\\\\'; \
         v=$(dd bs=1 count=25 2>/dev/null); printf 'got:%s\\n' \"$v\" | tr -d '\\033'; sleep 30",
    );
    wait_for_text(&mut connection, &terminal, "ready");
    let (channel, _) = attach(&mut connection, &terminal, json!({}));
    connection.until_meta(channel, "snapshot_end");
    // sent first, as pinrail attach does, while still an observer
    connection.send(
        channel,
        kind::FACTS,
        json!({"background": "#102030", "foreground": "#eeeeee"})
            .to_string()
            .into_bytes(),
    );
    connection.send(channel, kind::CONTROL, b"\"take\"".to_vec());
    let text = {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let reply = connection.call("terminal.text", json!({"terminal": terminal, "lines": 0}));
            let text = reply["result"]["text"].as_str().unwrap_or("").to_string();
            if text.contains("got:") || Instant::now() > deadline {
                break text;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    assert!(text.contains("rgb:1010/2020/3030"), "{text:?}");
    connection.call("terminal.stop", json!({"terminal": terminal}));
}

#[test]
fn a_large_snapshot_does_not_hold_back_another_terminal_s_output() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut setup = Connection::open(&host);
    let long = spawn(
        &mut setup,
        &folders,
        "seq 1 6000 | awk '{printf \"\\033[3%dmline %d of a long history, with text to make each line about as long as agents write them\\033[0m\\n\", $1 % 8, $1}'; sleep 60",
    );
    wait_for_text(&mut setup, &long, "line 6000");
    let quiet = spawn(&mut setup, &folders, "sleep 2; echo tick; sleep 60");

    // a second connection, which reads nothing until the long terminal's
    // snapshot and the quiet terminal's output are both queued
    let mut stream = UnixStream::connect(&host.registration.socket).unwrap();
    let mut call = |id: u64, method: &str, params: Value| {
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        frame::write_blocking(&mut stream, &Frame::json(&message)).unwrap();
    };
    call(0, "hello", json!({"secret": host.registration.secret}));
    let attach = |terminal: &str, history: u32| {
        json!({"terminal": terminal, "mode": "observe", "cols": 60, "rows": 12,
               "history": history, "formats": ["vt"]})
    };
    call(1, "terminal.attach", attach(&quiet, 0));
    std::thread::sleep(Duration::from_millis(300));
    call(2, "terminal.attach", attach(&long, 6000));
    std::thread::sleep(Duration::from_millis(2500));

    // DATA bytes per channel, and as they were when the tick came
    let mut bytes: BTreeMap<u32, usize> = BTreeMap::new();
    let mut at_tick: Option<BTreeMap<u32, usize>> = None;
    let mut ended = false;
    let deadline = Instant::now() + Duration::from_secs(20);
    while !(ended && at_tick.is_some()) {
        assert!(
            Instant::now() < deadline,
            "no tick or snapshot end: {bytes:?}"
        );
        let frame = frame::read_blocking(&mut stream).unwrap().unwrap();
        if frame.channel == 0 {
            continue;
        }
        match frame.kind {
            kind::DATA => {
                *bytes.entry(frame.channel).or_default() += frame.payload.len();
                // credit for what was read, as the core returns it
                let credit = Frame {
                    channel: frame.channel,
                    kind: kind::CREDIT,
                    payload: (frame.payload.len() as u32).to_le_bytes().to_vec(),
                };
                frame::write_blocking(&mut stream, &credit).unwrap();
                if frame.payload.windows(4).any(|w| w == b"tick") {
                    at_tick.get_or_insert(bytes.clone());
                }
            }
            kind::META => {
                let meta: Value = serde_json::from_slice(&frame.payload).unwrap();
                let big = bytes.get(&frame.channel).is_some_and(|&n| n > 64 * 1024);
                ended |= big && meta["type"] == "snapshot_end";
            }
            _ => {}
        }
    }
    let (&long_channel, &snapshot) = bytes.iter().max_by_key(|(_, n)| **n).unwrap();
    let before_tick = at_tick.unwrap().get(&long_channel).copied().unwrap_or(0);
    assert!(snapshot > 256 * 1024, "the snapshot was {snapshot} bytes");
    // the quiet terminal's output took its turn early in the snapshot
    assert!(
        before_tick < 128 * 1024,
        "{before_tick} of the snapshot's {snapshot} bytes came before the tick"
    );
}
