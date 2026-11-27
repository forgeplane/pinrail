//! Terminals on Windows, through a real host and holder: a program runs on
//! a pseudo console, its output and exit come back, what is typed reaches
//! it, Ctrl+C and a stop end it, the program in front is reported, and a
//! host that is killed ends no program: the next one takes it back.
#![cfg(windows)]

mod common;

use std::time::{Duration, Instant};

use pinrail_host::client::Client;
use serde_json::{Value, json};

use common::{Host, folders, serve};

fn connect(host: &Host) -> Client {
    Client::connect(&host.registration).unwrap()
}

/// Spawns `argv` in `cwd`, with the host's environment.
fn spawn(client: &mut Client, argv: &[&str], cwd: &std::path::Path) -> (String, u64) {
    let spawned = client
        .call(
            "terminal.spawn",
            json!({"argv": argv, "cwd": cwd, "cols": 80, "rows": 24,
                   "env": {}, "env_base": "host", "labels": {"run": "run_1"}}),
        )
        .unwrap();
    (
        spawned["terminal"].as_str().unwrap().to_string(),
        spawned["pid"].as_u64().unwrap(),
    )
}

fn text(client: &mut Client, terminal: &str, lines: u64) -> String {
    client
        .call(
            "terminal.text",
            json!({"terminal": terminal, "lines": lines}),
        )
        .unwrap()["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn wait_for(client: &mut Client, terminal: &str, wanted: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let screen = text(client, terminal, 100);
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

/// The terminal's entry once it has ended.
fn ended(client: &mut Client, terminal: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(entry) = listed(client)
            .into_iter()
            .find(|t| t["terminal"] == terminal)
            .filter(|t| t.get("ended_at").is_some())
        {
            return entry;
        }
        assert!(Instant::now() < deadline, "the terminal did not end");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn submit(client: &mut Client, terminal: &str, line: &str) {
    client
        .call(
            "terminal.submit",
            json!({"terminal": terminal, "text": line, "bracketed": false, "anyway": true}),
        )
        .unwrap();
}

#[test]
fn a_program_runs_on_a_pseudo_console_and_its_last_screen_and_exit_are_kept() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = connect(&host);
    let (id, pid) = spawn(
        &mut client,
        &[
            "cmd",
            "/d",
            "/c",
            "echo id=%PINRAIL_TERMINAL_ID%& echo term=%TERM%& exit 4",
        ],
        &folders.runtime,
    );
    assert!(pid > 0);
    let entry = ended(&mut client, &id);
    assert_eq!(entry["exit"], json!({"code": 4}));
    let screen = text(&mut client, &id, 0);
    assert!(screen.contains(&format!("id={id}")), "{screen}");
    assert!(screen.contains("term=xterm-256color"), "{screen}");
}

#[test]
fn the_exit_never_overtakes_the_program_s_last_output() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = connect(&host);
    let (id, _) = spawn(
        &mut client,
        &[
            "cmd",
            "/d",
            "/c",
            "for /L %i in (1,1,3000) do @echo line %i",
        ],
        &folders.runtime,
    );
    assert_eq!(ended(&mut client, &id)["exit"], json!({"code": 0}));
    assert!(text(&mut client, &id, 0).contains("line 3000"));
    assert!(text(&mut client, &id, 100).contains("line 2950"));
}

#[test]
fn what_is_typed_reaches_the_program_and_ctrl_c_interrupts_it() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = connect(&host);
    let (id, _) = spawn(&mut client, &["cmd", "/d", "/q"], &folders.runtime);
    submit(&mut client, &id, "echo typed-%TERM%");
    wait_for(&mut client, &id, "typed-xterm-256color");
    // a program that runs until it is interrupted
    submit(&mut client, &id, "ping -t 127.0.0.1");
    wait_for(&mut client, &id, "Reply from 127.0.0.1");
    client
        .call("terminal.signal", json!({"terminal": id, "signal": "INT"}))
        .unwrap();
    // ping prints its statistics when it is interrupted, and cmd goes on
    wait_for(&mut client, &id, "Ping statistics");
    submit(&mut client, &id, "echo after-%TERM%");
    wait_for(&mut client, &id, "after-xterm-256color");
    client
        .call("terminal.stop", json!({"terminal": id}))
        .unwrap();
    ended(&mut client, &id);
}

#[test]
fn a_stop_ends_a_program_and_everything_it_started() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = connect(&host);
    let (id, pid) = spawn(
        &mut client,
        &["cmd", "/d", "/c", "ping -n 600 127.0.0.1"],
        &folders.runtime,
    );
    wait_for(&mut client, &id, "Reply from 127.0.0.1");
    // ping, which cmd started
    let child = (0..50)
        .find_map(|_| {
            std::thread::sleep(Duration::from_millis(50));
            running_child_of(pid as u32)
        })
        .expect("ping runs under cmd");
    client
        .call("terminal.stop", json!({"terminal": id, "grace_ms": 1000}))
        .unwrap();
    let entry = ended(&mut client, &id);
    assert!(entry["exit"].is_object(), "{entry}");
    let deadline = Instant::now() + Duration::from_secs(10);
    while pinrail_runtime::process::process(child).is_some() {
        assert!(Instant::now() < deadline, "ping outlived its terminal");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The ping whose parent is `pid`.
fn running_child_of(pid: u32) -> Option<u32> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("(Get-CimInstance Win32_Process -Filter 'ParentProcessId={pid}').ProcessId"),
        ])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .find(|child| pinrail_runtime::process::process(*child).is_some_and(|p| p.name == "ping"))
}

#[test]
fn the_program_in_front_of_a_shell_is_reported() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = connect(&host);
    client.call("events.subscribe", json!({})).unwrap();
    let cwd = folders.runtime.canonicalize().unwrap();
    let (id, _) = spawn(&mut client, &["cmd", "/d", "/q"], &cwd);
    submit(&mut client, &id, "ping -n 30 127.0.0.1");
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut seen: Vec<Value> = Vec::new();
    while Instant::now() < deadline && seen.last().is_none_or(|l| l["name"] != "ping") {
        if let Some(message) = client.notification(Duration::from_millis(200))
            && message["method"] == "foreground"
            && message["params"]["terminal"] == id.as_str()
        {
            seen.push(message["params"].clone());
        }
    }
    let last = seen.last().expect("a foreground event");
    assert_eq!(last["name"], "ping", "{seen:?}");
    assert_eq!(
        last["args"],
        json!(["ping", "-n", "30", "127.0.0.1"]),
        "{seen:?}"
    );
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

#[test]
fn a_new_host_adopts_the_programs_of_a_host_that_was_killed() {
    let folders = folders();
    let mut first = serve(&folders, &[]);
    let mut client = connect(&first);
    let (id, pid) = spawn(&mut client, &["cmd", "/d", "/q"], &folders.runtime);
    submit(&mut client, &id, "echo ready");
    wait_for(&mut client, &id, "ready");

    // the host dies without a word
    let _ = first.child.kill();
    let _ = first.child.wait();
    drop(client);
    let holders = first.holders.clone();
    first.holders = folders.data.join("none");
    drop(first);

    let second = serve(&folders, &[]);
    let mut client = connect(&second);
    let terminals = listed(&mut client);
    assert_eq!(terminals.len(), 1, "{terminals:?}");
    assert_eq!(terminals[0]["terminal"], id.as_str());
    assert_eq!(terminals[0]["pid"], pid);
    // the screen, rebuilt from the output ring
    wait_for(&mut client, &id, "ready");
    // the program still reads what is typed, and the host what it writes
    submit(&mut client, &id, "echo got-hello");
    wait_for(&mut client, &id, "got-hello");
    assert_eq!(second.holders, holders);
    client
        .call("terminal.stop", json!({"terminal": id}))
        .unwrap();
    ended(&mut client, &id);
}
