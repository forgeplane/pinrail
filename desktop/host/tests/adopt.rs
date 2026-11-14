//! A host that is killed ends no agent: the holder keeps the program and
//! its PTY, and the next host takes the terminal back, with its screen
//! rebuilt from the output ring, and goes on reading and typing.
#![cfg(unix)]

mod common;

use std::time::{Duration, Instant};

use pinrail_host::client::Client;
use serde_json::{Value, json};

use common::{folders, serve};

fn connect(host: &common::Host) -> Client {
    Client::connect(&host.registration).unwrap()
}

fn text(client: &mut Client, terminal: &str) -> String {
    client
        .call("terminal.text", json!({"terminal": terminal, "lines": 100}))
        .unwrap()["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn wait_for(client: &mut Client, terminal: &str, wanted: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let screen = text(client, terminal);
        if screen.contains(wanted) {
            return screen;
        }
        assert!(Instant::now() < deadline, "no {wanted:?} in {screen:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn listed(client: &mut Client) -> Vec<Value> {
    client
        .call("terminal.list", json!({}))
        .unwrap()
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[test]
fn a_new_host_adopts_the_agents_of_a_host_that_was_killed() {
    let folders = folders();
    let mut first = serve(&folders, &[]);
    let mut client = connect(&first);
    let spawned = client
        .call(
            "terminal.spawn",
            json!({"argv": ["sh", "-c", "echo ready; while read line; do echo got:$line; done"],
                   "cwd": folders.runtime, "cols": 80, "rows": 24,
                   "env": {"PATH": "/usr/bin:/bin"}, "env_base": "host",
                   "labels": {"run": "run_1"}}),
        )
        .unwrap();
    let terminal = spawned["terminal"].as_str().unwrap().to_string();
    let pid = spawned["pid"].as_u64().unwrap();
    wait_for(&mut client, &terminal, "ready");

    // the host dies without a word
    let _ = first.child.kill();
    let _ = first.child.wait();
    drop(client);
    let holders = first.holders.clone();
    // the next host must not kill the holders when this one is dropped
    first.holders = folders.data.join("none");
    drop(first);

    let second = serve(&folders, &[]);
    let mut client = connect(&second);
    let terminals = listed(&mut client);
    assert_eq!(terminals.len(), 1, "{terminals:?}");
    assert_eq!(terminals[0]["terminal"], terminal.as_str());
    assert_eq!(terminals[0]["pid"], pid);
    assert_eq!(terminals[0]["labels"]["run"], "run_1");
    // the screen, rebuilt from the output ring
    wait_for(&mut client, &terminal, "ready");

    // the program still reads what is typed, and the host what it writes
    client
        .call(
            "terminal.submit",
            json!({"terminal": terminal, "text": "hello", "bracketed": false, "anyway": true}),
        )
        .unwrap();
    wait_for(&mut client, &terminal, "got:hello");
    assert_eq!(second.holders, holders);
}

#[test]
fn a_program_the_holder_keeps_without_a_record_is_stopped() {
    let folders = folders();
    let mut first = serve(&folders, &[]);
    let mut client = connect(&first);
    let spawned = client
        .call(
            "terminal.spawn",
            json!({"argv": ["sleep", "600"], "cwd": folders.runtime, "cols": 80, "rows": 24,
                   "env_base": "host"}),
        )
        .unwrap();
    let pid = spawned["pid"].as_u64().unwrap() as u32;
    let _ = first.child.kill();
    let _ = first.child.wait();
    drop(client);
    first.holders = folders.data.join("none");
    drop(first);
    // a spawn whose record the host never wrote, as after a crash in the
    // middle of one
    std::fs::write(folders.data.join("host/terminals.json"), "[]").unwrap();

    let second = serve(&folders, &[]);
    let mut client = connect(&second);
    assert!(listed(&mut client).is_empty());
    let deadline = Instant::now() + Duration::from_secs(10);
    while pinrail_runtime::process::process(pid).is_some_and(|p| p.name == "sleep") {
        assert!(Instant::now() < deadline, "the program kept running");
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(second);
}

#[test]
fn the_holder_starts_a_host_when_none_comes_back() {
    let folders = folders();
    let mut first = serve(&folders, &[]);
    let mut client = connect(&first);
    let spawned = client
        .call(
            "terminal.spawn",
            json!({"argv": ["sh", "-c", "echo ready; while read line; do echo got:$line; done"],
                   "cwd": folders.runtime, "cols": 80, "rows": 24,
                   "env": {"PATH": "/usr/bin:/bin"}, "env_base": "host"}),
        )
        .unwrap();
    let terminal = spawned["terminal"].as_str().unwrap().to_string();
    wait_for(&mut client, &terminal, "ready");
    let old = first.registration.pid;
    let _ = first.child.kill();
    let _ = first.child.wait();
    drop(client);
    let holders = first.holders.clone();
    first.holders = folders.data.join("none");
    drop(first);

    // nobody starts a host: the holder does, within about 5 seconds
    let path = common::registration_path(&folders);
    let deadline = Instant::now() + Duration::from_secs(20);
    let registration = loop {
        if let Some(registration) = pinrail_host::registration::Registration::read(&path)
            && registration.pid != old
            && Client::connect(&registration).is_ok()
        {
            break registration;
        }
        assert!(Instant::now() < deadline, "no host was started");
        std::thread::sleep(Duration::from_millis(100));
    };
    let mut client = Client::connect(&registration).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while listed(&mut client).is_empty() {
        assert!(Instant::now() < deadline, "the new host adopted nothing");
        std::thread::sleep(Duration::from_millis(50));
    }
    wait_for(&mut client, &terminal, "ready");
    // the host the holder started, and the holder, end with the test
    let _ = std::process::Command::new("kill")
        .arg(registration.pid.to_string())
        .status();
    common::kill_holders(&holders);
}
