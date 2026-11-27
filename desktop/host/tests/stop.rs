//! Signals, stops and the events a terminal reports: a stop escalates
//! until the program ends, every exit says why, and what the program tells
//! its terminal becomes an event.

mod common;

use std::time::{Duration, Instant};

use pinrail_host::client::{CallError, Client};
use serde_json::{Value, json};

use common::{Folders, folders, serve};

fn spawn(client: &mut Client, folders: &Folders, script: &str) -> (String, u64) {
    spawn_program(client, folders, common::shell(script))
}

fn spawn_program(client: &mut Client, folders: &Folders, argv: Vec<String>) -> (String, u64) {
    let spawned = client
        .call(
            "terminal.spawn",
            json!({
                "argv": argv,
                "cwd": folders.runtime,
                "env": {"PATH": common::path()},
                "cols": 80,
                "rows": 24,
                "labels": {},
            }),
        )
        .unwrap();
    (
        spawned["terminal"].as_str().unwrap().to_string(),
        spawned["pid"].as_u64().unwrap(),
    )
}

fn subscribed(client: &mut Client) {
    let reply = client.call("events.subscribe", json!({})).unwrap();
    assert!(reply["instance"].is_string());
}

/// The next event called `method` for `terminal`, skipping others.
fn event(client: &mut Client, method: &str, terminal: &str) -> Value {
    let deadline = Instant::now() + common::within(10);
    while Instant::now() < deadline {
        if let Some(message) = client.notification(Duration::from_millis(200))
            && message["method"] == method
            && message["params"]["terminal"] == terminal
        {
            return message["params"].clone();
        }
    }
    panic!("no {method} event for {terminal}");
}

/// Waits until the program has started and set its traps.
fn ready(client: &mut Client, terminal: &str) {
    let deadline = Instant::now() + common::within(5);
    loop {
        let text = client
            .call("terminal.text", json!({"terminal": terminal, "lines": 0}))
            .unwrap();
        if text["text"].as_str().unwrap().contains("ready") {
            return;
        }
        assert!(Instant::now() < deadline, "the program did not start");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn gone(pid: u64) -> bool {
    let deadline = Instant::now() + common::within(5);
    while Instant::now() < deadline {
        if !common::alive(pid as u32) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn a_stop_escalates_to_kill_for_a_program_that_ignores_hangup_and_terminate() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    subscribed(&mut client);
    #[cfg(unix)]
    let id = {
        let (id, _) = spawn(
            &mut client,
            &folders,
            "trap '' HUP TERM; echo ready; while :; do sleep 0.05; done",
        );
        ready(&mut client, &id);
        id
    };
    // On Windows a hangup and a terminate close the console, which ends
    // every program on it whatever it asked for. What they leave running
    // is a program that has no console, as one with a window: here the
    // script host for windows, with a script that waits.
    #[cfg(windows)]
    let id = {
        std::fs::write(folders.runtime.join("stay.js"), "WScript.Sleep(300000);").unwrap();
        let argv = ["wscript", "//B", "//Nologo", "stay.js"].map(String::from);
        spawn_program(&mut client, &folders, argv.to_vec()).0
    };
    let started = Instant::now();
    client
        .call("terminal.stop", json!({"terminal": id, "grace_ms": 300}))
        .unwrap();
    let exited = event(&mut client, "exited", &id);
    assert_eq!(exited["reason"], "stopped");
    #[cfg(unix)]
    assert_eq!(exited["signal"], 9);
    // the code of a program whose job was ended
    #[cfg(windows)]
    assert_eq!(exited["code"], 1);
    // 500 ms after the hangup, then the grace
    assert!(started.elapsed() >= Duration::from_millis(800));
}

#[test]
fn a_stop_ends_at_terminate_for_a_program_that_ignores_only_hangup() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    subscribed(&mut client);
    let (id, _) = spawn(
        &mut client,
        &folders,
        "trap '' HUP; echo ready; while :; do sleep 0.05; done",
    );
    ready(&mut client, &id);
    let started = Instant::now();
    client
        .call("terminal.stop", json!({"terminal": id, "grace_ms": 5000}))
        .unwrap();
    let exited = event(&mut client, "exited", &id);
    #[cfg(unix)]
    assert_eq!(
        (exited["reason"].clone(), exited["signal"].clone()),
        (json!("stopped"), json!(15))
    );
    // On Windows the hangup and the terminate are one act, the closing of
    // the console, which no program on it can ignore: it ends with a code,
    // as every program does there.
    #[cfg(windows)]
    {
        assert_eq!(exited["reason"], "stopped");
        assert!(exited["code"].is_number() && exited["signal"].is_null());
    }
    // well within the grace, after which it would have been killed
    assert!(started.elapsed() < Duration::from_millis(5_000));
}

#[test]
fn a_stop_ends_the_program_s_children_too() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    subscribed(&mut client);
    let (id, _) = spawn(
        &mut client,
        &folders,
        "sleep 300 & echo \"child=$!\"; echo ready; wait",
    );
    ready(&mut client, &id);
    let text = client
        .call("terminal.text", json!({"terminal": id, "lines": 0}))
        .unwrap();
    let child: u64 = text["text"]
        .as_str()
        .unwrap()
        .lines()
        .find_map(|l| l.trim().strip_prefix("child="))
        .unwrap()
        .parse()
        .unwrap();
    client
        .call("terminal.stop", json!({"terminal": id}))
        .unwrap();
    event(&mut client, "exited", &id);
    assert!(gone(child), "the child {child} still runs");
}

#[test]
fn every_exit_says_why() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    subscribed(&mut client);
    let (id, _) = spawn(&mut client, &folders, "exit 5");
    let exited = event(&mut client, "exited", &id);
    assert_eq!(
        (exited["reason"].clone(), exited["code"].clone()),
        (json!("exited"), json!(5))
    );

    let (id, _) = spawn(&mut client, &folders, "kill -TERM $$");
    let exited = event(&mut client, "exited", &id);
    #[cfg(unix)]
    assert_eq!(exited["reason"], "signaled");
    // Windows has no signals to die of: Git's shell, which has its own,
    // exits with the signal in its code
    #[cfg(windows)]
    assert_eq!(
        (exited["reason"].clone(), exited["code"].clone()),
        (json!("exited"), json!(15 << 8))
    );

    let (id, _) = spawn(
        &mut client,
        &folders,
        "trap 'exit 9' INT; echo ready; while :; do sleep 0.05; done",
    );
    ready(&mut client, &id);
    client
        .call("terminal.signal", json!({"terminal": id, "signal": "INT"}))
        .unwrap();
    let exited = event(&mut client, "exited", &id);
    assert_eq!(
        (exited["reason"].clone(), exited["code"].clone()),
        (json!("exited"), json!(9))
    );
    // an ended terminal takes no more signals
    let error = client
        .call("terminal.signal", json!({"terminal": id, "signal": "INT"}))
        .unwrap_err();
    assert!(matches!(error, CallError::Rpc(e) if e["data"]["code"] == "exited"));
    let error = client
        .call("terminal.signal", json!({"terminal": id, "signal": "USR1"}))
        .unwrap_err();
    assert!(matches!(error, CallError::Rpc(e) if e["data"]["code"] == "invalid"));
}

#[test]
fn what_the_program_tells_its_terminal_becomes_events() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    subscribed(&mut client);
    let long = "x".repeat(300);
    let (id, pid) = spawn(
        &mut client,
        &folders,
        &format!(
            "printf '\\033]0;⠋ Fixing\\007'; printf '\\033]0;⠙ Fixing\\007'; \
             printf '\\033]0;Done\\007'; printf '\\007'; \
             printf '\\033]777;notify;Codex;{long}\\007'; \
             printf '\\033]7501;state=blocked:kind=question:id=r1\\033\\\\'; sleep 0.2"
        ),
    );
    let spawned = event(&mut client, "spawned", &id);
    assert_eq!(spawned["pid"], pid);
    // the spinner's frames are one title
    assert_eq!(event(&mut client, "title", &id)["text"], "⠋ Fixing");
    assert_eq!(event(&mut client, "title", &id)["text"], "Done");
    event(&mut client, "bell", &id);
    let notify = event(&mut client, "notify", &id);
    assert_eq!(notify["title"], "Codex");
    assert_eq!(notify["body"].as_str().unwrap().chars().count(), 240);
    let status = event(&mut client, "program_status", &id);
    assert_eq!(
        (
            status["state"].clone(),
            status["kind"].clone(),
            status["id"].clone()
        ),
        (json!("blocked"), json!("question"), json!("r1"))
    );
    let exited = event(&mut client, "exited", &id);
    // seq increases
    assert!(exited["seq"].as_u64() > status["seq"].as_u64());
}

#[test]
fn an_ended_terminal_keeps_its_record_and_final_screen() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    subscribed(&mut client);
    let (id, pid) = spawn(&mut client, &folders, "echo 'last words'; exit 2");
    event(&mut client, "exited", &id);
    let screen = folders.data.join(format!("host/screens/{id}.snap"));
    let paint = std::fs::read_to_string(&screen).unwrap();
    assert!(paint.contains("last words"));
    // its owner's alone; on Windows the host's folder makes it so
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&screen).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    // terminals.json catches up within a second or so
    let deadline = Instant::now() + common::within(5);
    loop {
        let records: Value = serde_json::from_str(
            &std::fs::read_to_string(folders.data.join("host/terminals.json")).unwrap(),
        )
        .unwrap();
        let record = records
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id.as_str())
            .unwrap()
            .clone();
        if record["exit"] == json!({"code": 2}) {
            assert_eq!(record["pid"], pid);
            assert_eq!(record["argv0"], common::shell("")[0].as_str());
            assert!(record["ended_at"].is_string());
            break;
        }
        assert!(Instant::now() < deadline, "{record}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn stopping_the_host_stops_its_terminals() {
    let folders = folders();
    let mut host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    subscribed(&mut client);
    let (id, pid) = spawn(&mut client, &folders, "echo ready; sleep 300");
    ready(&mut client, &id);
    client
        .call("host.stop", json!({"stop_terminals": true}))
        .unwrap();
    assert_eq!(event(&mut client, "exited", &id)["reason"], "host_shutdown");
    assert!(gone(pid));
    assert!(common::exits_within(&mut host, common::within(5)));
}

/// The records in the host's `terminals.json`.
fn records(folders: &common::Folders) -> Vec<Value> {
    std::fs::read_to_string(folders.data.join("host/terminals.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

#[test]
fn a_host_that_stops_leaves_the_exits_in_its_records() {
    let folders = folders();
    let mut host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    let (id, _) = spawn(&mut client, &folders, "echo ready; sleep 300");
    ready(&mut client, &id);
    client
        .call("host.stop", json!({"stop_terminals": true}))
        .unwrap();
    assert!(common::exits_within(&mut host, common::within(5)));
    let records = records(&folders);
    assert_eq!(records[0]["id"], id.as_str());
    assert!(records[0]["ended_at"].is_string(), "{records:?}");
    assert!(records[0]["exit"].is_object(), "{records:?}");
}

#[test]
fn a_host_that_starts_ends_the_records_of_terminals_no_holder_keeps() {
    let folders = folders();
    // what a host that was killed as it stopped its terminals leaves
    std::fs::create_dir_all(folders.data.join("host")).unwrap();
    std::fs::write(
        folders.data.join("host/terminals.json"),
        json!([{"id": "t_gone", "holder": "0a0a0a0a", "pid": 1, "labels": {},
                "started_at": "2026-01-01T00:00:00Z", "ended_at": null, "exit": null}])
        .to_string(),
    )
    .unwrap();
    let host = serve(&folders, &[]);
    let deadline = Instant::now() + common::within(5);
    loop {
        let records = records(&folders);
        if records.len() == 1 && records[0]["ended_at"].is_string() {
            break;
        }
        assert!(Instant::now() < deadline, "{records:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(host);
}

#[test]
fn a_subscriber_that_resumes_gets_only_what_it_missed() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    let first = client.call("events.subscribe", json!({})).unwrap();
    let instance = first["instance"].as_str().unwrap().to_string();
    let (id, _) = spawn(&mut client, &folders, "exit 0");
    let exited = event(&mut client, "exited", &id);
    let spawned_seq = exited["seq"].as_u64().unwrap() - 1;
    drop(client);

    // a new connection resumes after the spawn, so it gets the exit again
    let mut client = Client::connect(&host.registration).unwrap();
    let again = client
        .call(
            "events.subscribe",
            json!({"after": spawned_seq, "instance": instance}),
        )
        .unwrap();
    assert_eq!(again["terminals"][0]["terminal"], id.as_str());
    let message = client.notification(Duration::from_secs(2)).unwrap();
    assert_eq!(message["method"], "exited");
    // an `after` from another instance replays nothing
    drop(client);
    let mut client = Client::connect(&host.registration).unwrap();
    client
        .call(
            "events.subscribe",
            json!({"after": 0, "instance": "another"}),
        )
        .unwrap();
    assert!(client.notification(Duration::from_millis(300)).is_none());
}
