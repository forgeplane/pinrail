//! `terminal.submit` types a message into the program: after a starting
//! program goes quiet, inside a paste its text cannot end, then the submit
//! keys once the output settles, with the person's keys held meanwhile.
#![cfg(unix)]

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use pinrail_host::client::{CallError, Client};
use pinrail_host::events::Events;
use pinrail_host::pty;
use pinrail_host::records::Records;
use pinrail_host::terminal::{self, Context, Input, Submit, SubmitError, Terminal};
use serde_json::{Value, json};

use common::{Folders, folders, serve};

/// A program that reads `count` bytes raw and shows them as `od -c` does.
fn reader(count: usize, before: &str) -> String {
    format!(
        "stty raw -echo; {before} echo ready; \
         r=$(dd bs=1 count={count} 2>/dev/null | od -An -c | tr -s ' \\n' ' '); \
         printf '\\r\\ngot:%s\\r\\n' \"$r\"; sleep 5"
    )
}

fn spawn(client: &mut Client, folders: &Folders, script: &str) -> (String, u64) {
    let spawned = client
        .call(
            "terminal.spawn",
            json!({
                "argv": ["sh", "-c", script],
                "cwd": folders.runtime,
                "cols": 120,
                "rows": 24,
                "env_base": "host",
            }),
        )
        .unwrap();
    (
        spawned["terminal"].as_str().unwrap().to_string(),
        spawned["pid"].as_u64().unwrap(),
    )
}

fn screen_shows(client: &mut Client, id: &str, text: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let screen = client
            .call("terminal.text", json!({"terminal": id, "lines": 0}))
            .unwrap()["text"]
            .as_str()
            .unwrap()
            .to_string();
        if screen.contains(text) || Instant::now() > deadline {
            return screen;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn code(error: CallError) -> String {
    match error {
        CallError::Rpc(error) => error["data"]["code"].as_str().unwrap().to_string(),
        other => panic!("{other}"),
    }
}

#[test]
fn the_message_is_pasted_without_its_end_marker_then_submitted() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    // ESC [ 200 ~, "hi", ESC [ 201 ~, CR
    let (id, pid) = spawn(&mut client, &folders, &reader(15, ""));
    screen_shows(&mut client, &id, "ready");
    let typed = client
        .call(
            "terminal.submit",
            json!({
                "terminal": id,
                "text": "h\u{1b}[201~i",
                "bracketed": true,
                "settle_ms": 50,
                "max_ms": 1000,
                "startup_quiet_ms": 0,
                "submit": "\r",
                "expect_pid": pid,
            }),
        )
        .unwrap();
    assert_eq!(typed, json!({"ok": true}));
    let screen = screen_shows(&mut client, &id, "got:");
    assert!(
        screen.contains("got: 033 [ 2 0 0 ~ h i 033 [ 2 0 1 ~ \\r"),
        "{screen}"
    );
}

#[test]
fn a_starting_program_is_left_to_go_quiet_first() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    // draws for a second before it reads
    let (id, _) = spawn(
        &mut client,
        &folders,
        &reader(
            3,
            "i=0; while [ $i -lt 20 ]; do echo drawing; sleep 0.05; i=$((i+1)); done;",
        ),
    );
    let started = Instant::now();
    client
        .call(
            "terminal.submit",
            json!({
                "terminal": id,
                "text": "ok",
                "bracketed": false,
                "settle_ms": 0,
                "max_ms": 0,
                "startup_quiet_ms": 300,
            }),
        )
        .unwrap();
    assert!(
        started.elapsed() >= Duration::from_millis(1_000),
        "{:?}",
        started.elapsed()
    );
    let screen = screen_shows(&mut client, &id, "got:");
    assert!(screen.contains("got: o k \\r"), "{screen}");
}

#[test]
fn a_message_is_refused_when_another_program_holds_the_terminal_or_it_ended() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    let (id, pid) = spawn(&mut client, &folders, "echo ready; sleep 30");
    screen_shows(&mut client, &id, "ready");
    let wrong = client
        .call(
            "terminal.submit",
            json!({"terminal": id, "text": "x", "expect_pid": pid + 100_000}),
        )
        .unwrap_err();
    assert_eq!(code(wrong), "not_foreground");

    let (ended, _) = spawn(&mut client, &folders, "exit 0");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let list: Value = client.call("terminal.list", json!({})).unwrap();
        if list
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["terminal"] == ended.as_str() && t.get("ended_at").is_some())
        {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let refused = client
        .call("terminal.submit", json!({"terminal": ended, "text": "x"}))
        .unwrap_err();
    assert_eq!(code(refused), "exited");
    client
        .call("terminal.stop", json!({"terminal": id}))
        .unwrap();
}

/// A terminal started without a host.
fn terminal(script: &str) -> (Terminal, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let context = Context {
        events: Arc::new(Events::default()),
        records: Arc::new(Records::new(dir.path().join("terminals.json"))),
        screens: dir.path().join("screens"),
        controlled: Arc::new(AtomicBool::new(false)),
    };
    let env: BTreeMap<String, String> = [("PATH".to_string(), "/usr/bin:/bin".to_string())].into();
    let argv = ["sh", "-c", script].map(String::from);
    let spawned = pty::spawn(&argv, dir.path(), &env, 120, 24).unwrap();
    let screen = terminal::screen(120, 24, 100).unwrap();
    let pid = spawned.pid;
    let terminal = Terminal::start(
        "t_test".to_string(),
        spawned,
        json!({}),
        (120, 24),
        screen,
        context,
        pinrail_host::terminal::Holding {
            forget: Box::new(move || pty::reap(pid)),
            ring: None,
        },
    )
    .unwrap();
    // the program is this process's child, not a holder's
    let report = terminal.exit_reporter();
    std::thread::spawn(move || report(pty::wait_exit(pid).unwrap_or(pty::Exit::Code(-1))));
    (terminal, dir)
}

async fn shows(terminal: &Terminal, text: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let screen = terminal.text_request(0).await.unwrap();
        if screen.contains(text) || Instant::now() > deadline {
            return screen;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn message(text: &str, settle: Duration) -> Submit {
    Submit {
        text: text.to_string(),
        bracketed: false,
        settle,
        max: Duration::from_secs(2),
        startup_quiet: Duration::ZERO,
        submit: "\r".to_string(),
        expect_pid: None,
        idle_since: None,
        anyway: false,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_person_s_keys_wait_until_the_message_is_submitted() {
    let (terminal, _dir) = terminal(&reader(5, ""));
    shows(&terminal, "ready").await;
    let typed = terminal.submit(message("ab", Duration::from_millis(300)));
    // the person types while the message settles
    tokio::time::sleep(Duration::from_millis(100)).await;
    terminal.input(b"Z".to_vec(), Input::Person);
    terminal.input(b"Y".to_vec(), Input::Person);
    typed.await.unwrap();
    let screen = shows(&terminal, "got:").await;
    assert!(screen.contains("got: a b \\r Z Y"), "{screen}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_waits_for_the_person_unless_sent_anyway() {
    let (terminal, _dir) = terminal(&reader(2, ""));
    shows(&terminal, "ready").await;
    let idle_since = chrono::Utc::now().to_rfc3339();
    tokio::time::sleep(Duration::from_millis(20)).await;
    terminal.input(b"q".to_vec(), Input::Person);
    let since_idle = Submit {
        idle_since: Some(idle_since.clone()),
        ..message("x", Duration::ZERO)
    };
    assert_eq!(
        terminal.submit(since_idle.clone()).await,
        Err(SubmitError::UsedSinceIdle)
    );
    // input before the agent became idle does not count
    let earlier = Submit {
        idle_since: Some(chrono::Utc::now().to_rfc3339()),
        ..message("x", Duration::ZERO)
    };
    assert_eq!(terminal.submit(earlier).await, Ok(()));
    let screen = shows(&terminal, "got:").await;
    assert!(screen.contains("got: q x"), "{screen}");
    // and Send anyway overrides the guard
    let (terminal, _dir) = terminal_again();
    shows(&terminal, "ready").await;
    terminal.input(b"q".to_vec(), Input::Person);
    let anyway = Submit {
        anyway: true,
        ..since_idle
    };
    assert_eq!(terminal.submit(anyway).await, Ok(()));
}

fn terminal_again() -> (Terminal, tempfile::TempDir) {
    terminal(&reader(3, ""))
}
