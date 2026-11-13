//! The host reports which program holds a terminal, and tells the
//! person's input from a viewer's replies.
#![cfg(unix)]

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use pinrail_host::client::Client;
use pinrail_host::events::Events;
use pinrail_host::frame::Frame;
use pinrail_host::outbox::Outbox;
use pinrail_host::pty;
use pinrail_host::records::Records;
use pinrail_host::terminal::{self, Context, Input, Terminal};
use serde_json::{Value, json};

use common::{folders, serve};

#[test]
fn the_program_that_holds_the_terminal_is_reported_when_it_changes() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    client.call("events.subscribe", json!({})).unwrap();
    let cwd = folders.runtime.canonicalize().unwrap();
    let spawned = client
        .call(
            "terminal.spawn",
            json!({
                "argv": ["sh", "-c", "sleep 1.2; exec sleep 5"],
                "cwd": cwd,
                "cols": 80,
                "rows": 24,
            }),
        )
        .unwrap();
    let id = spawned["terminal"].as_str().unwrap().to_string();
    let pid = spawned["pid"].as_u64().unwrap();
    // on macOS /bin/sh is a launcher that execs bash, so the program may
    // change from sh to bash before it becomes sleep
    let mut seen: Vec<Value> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline && seen.last().is_none_or(|l| l["name"] != "sleep") {
        if let Some(message) = client.notification(Duration::from_millis(200))
            && message["method"] == "foreground"
            && message["params"]["terminal"] == id.as_str()
        {
            seen.push(message["params"].clone());
        }
    }
    assert!(seen.len() >= 2, "{seen:?}");
    assert!(
        ["sh", "bash"].contains(&seen[0]["name"].as_str().unwrap()),
        "{seen:?}"
    );
    // the same process throughout, with each program reported once
    assert!(seen.iter().all(|l| l["pid"] == pid), "{seen:?}");
    assert!(
        seen.windows(2).all(|w| w[0]["name"] != w[1]["name"]),
        "{seen:?}"
    );
    let last = seen.last().unwrap();
    assert_eq!(last["name"], "sleep", "{seen:?}");
    assert_eq!(last["args"], json!(["sleep", "5"]));
    assert_eq!(
        std::path::Path::new(last["cwd"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        cwd
    );
    client
        .call("terminal.stop", json!({"terminal": id}))
        .unwrap();
}

/// A terminal started without a host, and the events it reports.
fn terminal(
    script: &str,
) -> (
    Terminal,
    tokio::sync::mpsc::UnboundedReceiver<Frame>,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().unwrap();
    let events = Arc::new(Events::default());
    // the events reach a connection's outbox; read them from its other end
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let (near, mut far) = tokio::io::duplex(1 << 20);
    let (out, _writer) = Outbox::new(near);
    events.subscribe(1, &out, None, |_| json!({}));
    tokio::spawn(async move {
        while let Ok(Some(frame)) = pinrail_host::frame::read(&mut far).await {
            if tx.send(frame).is_err() {
                break;
            }
        }
    });
    let context = Context {
        events,
        records: Arc::new(Records::new(dir.path().join("terminals.json"))),
        screens: dir.path().join("screens"),
        controlled: Arc::new(AtomicBool::new(false)),
    };
    let env: BTreeMap<String, String> = [("PATH".to_string(), "/usr/bin:/bin".to_string())].into();
    let argv = ["sh", "-c", script].map(String::from);
    let spawned = pty::spawn(&argv, dir.path(), &env, 80, 24).unwrap();
    let screen = terminal::screen(80, 24, 100).unwrap();
    let terminal = Terminal::start(
        "t_test".to_string(),
        spawned,
        json!({}),
        (80, 24),
        screen,
        context,
    )
    .unwrap();
    (terminal, rx, dir)
}

/// The events received so far, by method.
fn methods(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Frame>) -> Vec<Value> {
    let mut out = Vec::new();
    while let Ok(frame) = rx.try_recv() {
        let message: Value = serde_json::from_slice(&frame.payload).unwrap();
        if message.get("method").is_some() {
            out.push(message);
        }
    }
    out
}

async fn screen_shows(terminal: &Terminal, text: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let screen = terminal.text_request(0).await.unwrap();
        if screen.contains(text) || Instant::now() > deadline {
            return screen;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_person_s_input_reaches_the_program_and_counts_as_activity() {
    let (terminal, mut rx, _dir) = terminal(
        "stty raw -echo; echo ready; r=$(dd bs=1 count=5 2>/dev/null | od -An -c | tr -d ' '); \
         printf '\\r\\ngot:%s\\r\\n' \"$r\"; sleep 5",
    );
    screen_shows(&terminal, "ready").await;
    // a viewer's reply is written but is not the person's input
    terminal.input(b"ab".to_vec(), Input::Reply);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(terminal.last_input_at(), None);
    terminal.input(b"cd".to_vec(), Input::Person);
    terminal.input(b"\x03".to_vec(), Input::Person);
    let screen = screen_shows(&terminal, "got:").await;
    // in order, with the Ctrl-C as a byte, since the program set raw mode
    assert!(screen.contains("got:abcd003"), "{screen}");
    assert!(terminal.last_input_at().is_some());
    let interrupts: Vec<Value> = methods(&mut rx)
        .into_iter()
        .filter(|m| m["method"] == "input")
        .collect();
    assert_eq!(interrupts.len(), 1, "{interrupts:?}");
    assert_eq!(
        interrupts[0]["params"],
        json!({"terminal": "t_test", "kind": "interrupt", "seq": interrupts[0]["params"]["seq"]})
    );
    terminal
        .stop(Duration::from_millis(100), terminal::Reason::Stopped)
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_program_that_asks_for_focus_reports_is_told_the_truth() {
    // reads the report it gets on asking, then one more
    let (terminal, _rx, _dir) = terminal(
        "stty raw -echo; printf '\\033[?1004h'; \
         a=$(dd bs=1 count=3 2>/dev/null | od -An -c | tr -d ' \\n'); printf '\\r\\nfirst:%s\\r\\n' \"$a\"; \
         b=$(dd bs=1 count=3 2>/dev/null | od -An -c | tr -d ' \\n'); printf 'then:%s\\r\\n' \"$b\"; sleep 5",
    );
    // with no viewer, the terminal is not focused
    let screen = screen_shows(&terminal, "first:").await;
    assert!(screen.contains("first:033[O"), "{screen}");
    terminal.set_focus(true);
    let screen = screen_shows(&terminal, "then:").await;
    assert!(screen.contains("then:033[I"), "{screen}");
    // the same focus again tells nothing new
    terminal.set_focus(true);
    terminal
        .stop(Duration::from_millis(100), terminal::Reason::Stopped)
        .unwrap();
}
