//! Terminals through the protocol: `terminal.spawn`, `list` and `text`,
//! with the program's queries answered and its last output kept when it
//! exits.
#![cfg(unix)]

mod common;

use std::time::{Duration, Instant};

use pinrail_host::client::{CallError, Client};
use serde_json::{Value, json};

use common::{Host, folders, serve};

fn connect(host: &Host) -> Client {
    Client::connect(&host.registration).unwrap()
}

fn spawn(client: &mut Client, script: &str, cwd: &str) -> Result<Value, CallError> {
    client.call(
        "terminal.spawn",
        json!({
            "argv": ["sh", "-c", script],
            "cwd": cwd,
            "env": {"PATH": "/usr/bin:/bin"},
            "session_env": {},
            "cols": 80,
            "rows": 24,
            "labels": {"run": "run_1", "agent": "test"},
            "env_base": "host",
        }),
    )
}

/// The terminal's entry in `terminal.list` once it has ended.
fn ended(client: &mut Client, id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let list = client.call("terminal.list", json!({})).unwrap();
        let entry = list
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["terminal"] == id)
            .cloned()
            .unwrap();
        if entry.get("ended_at").is_some() {
            return entry;
        }
        assert!(Instant::now() < deadline, "the terminal did not end");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn text(client: &mut Client, id: &str, lines: u64) -> String {
    let text = client
        .call("terminal.text", json!({"terminal": id, "lines": lines}))
        .unwrap();
    text["text"].as_str().unwrap().to_string()
}

fn code(error: CallError) -> String {
    match error {
        CallError::Rpc(error) => error["data"]["code"].as_str().unwrap().to_string(),
        other => panic!("{other}"),
    }
}

#[test]
fn a_terminal_runs_its_program_and_keeps_its_last_screen() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = connect(&host);
    let cwd = folders.runtime.to_str().unwrap().to_string();
    let spawned = spawn(
        &mut client,
        "echo \"id=$PINRAIL_TERMINAL_ID\"; echo \"pwd=$PWD\"; echo \"term=$TERM\"; exit 4",
        &cwd,
    )
    .unwrap();
    let id = spawned["terminal"].as_str().unwrap();
    assert!(spawned["pid"].as_u64().unwrap() > 0);
    let entry = ended(&mut client, id);
    assert_eq!(entry["exit"], json!({"code": 4}));
    assert_eq!(entry["labels"], json!({"run": "run_1", "agent": "test"}));
    assert_eq!(
        (entry["cols"].as_u64(), entry["rows"].as_u64()),
        (Some(80), Some(24))
    );
    let screen = text(&mut client, id, 0);
    assert!(screen.contains(&format!("id={id}")), "{screen}");
    assert!(screen.contains(&format!("pwd={cwd}")), "{screen}");
    assert!(screen.contains("term=xterm-256color"), "{screen}");
}

#[test]
fn the_exit_never_overtakes_the_program_s_last_output() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = connect(&host);
    let cwd = folders.runtime.to_str().unwrap();
    let id = spawn(
        &mut client,
        "i=0; while [ $i -lt 20000 ]; do i=$((i+1)); echo \"line $i\"; done",
        cwd,
    )
    .unwrap()["terminal"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(ended(&mut client, &id)["exit"], json!({"code": 0}));
    let screen = text(&mut client, &id, 0);
    assert!(screen.contains("line 20000"), "{screen}");
    // and history before it
    let with_history = text(&mut client, &id, 100);
    assert!(with_history.contains("line 19950"));
}

#[test]
fn the_program_s_queries_are_answered() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = connect(&host);
    let id = spawn(
        &mut client,
        "stty -icanon -echo; printf '\\033[6n'; \
         r=$(dd bs=1 count=6 2>/dev/null | od -An -c | tr -d ' \\n'); \
         printf '\\nreply:%s\\n' \"$r\"",
        folders.runtime.to_str().unwrap(),
    )
    .unwrap()["terminal"]
        .as_str()
        .unwrap()
        .to_string();
    ended(&mut client, &id);
    let screen = text(&mut client, &id, 0);
    assert!(screen.contains("reply:033[1;1R"), "{screen}");
}

#[test]
fn a_spawn_that_cannot_start_answers_why() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = connect(&host);
    assert_eq!(
        code(spawn(&mut client, "true", "/no/such/folder").unwrap_err()),
        "folder_missing"
    );
    let missing = client
        .call(
            "terminal.spawn",
            json!({
                "argv": ["no-such-agent"],
                "cwd": "/",
                "env": {"PATH": "/usr/bin:/bin"},
                "cols": 80,
                "rows": 24,
            }),
        )
        .unwrap_err();
    assert_eq!(code(missing), "not_found");
    let no_size = client
        .call("terminal.spawn", json!({"argv": ["sh"], "cwd": "/"}))
        .unwrap_err();
    assert_eq!(code(no_size), "invalid_params");
    // nothing was started
    assert_eq!(client.call("terminal.list", json!({})).unwrap(), json!([]));
    let unknown = client
        .call("terminal.text", json!({"terminal": "t_none", "lines": 0}))
        .unwrap_err();
    assert_eq!(code(unknown), "not_found");
}

#[test]
fn a_live_terminal_counts_in_the_host_s_info() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = connect(&host);
    let id = spawn(&mut client, "sleep 30", folders.runtime.to_str().unwrap()).unwrap()["terminal"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(client.call("host.info", json!({})).unwrap()["terminals"], 1);
    let list = client.call("terminal.list", json!({})).unwrap();
    assert_eq!(list[0]["terminal"], id.as_str());
    assert!(list[0].get("ended_at").is_none());
}
