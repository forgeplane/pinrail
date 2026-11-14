//! The holder keeps the PTYs of the host's terminals open: it spawns a
//! program when a host that knows the secret asks, hands over the master
//! descriptor, reports the exit without reaping, and gives the
//! descriptors to the next host that connects, so a program outlives the
//! host that started it.
#![cfg(unix)]

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use pinrail_host::fdpass;
use pinrail_host::paths::Paths;
use pinrail_host::registration::Registration;
use serde_json::{Value, json};

use common::{Folders, command, folders};

const SECRET: &str = "holder-test-secret";

/// A holder of the test's folders, and its socket, with a registration
/// that holds the secret a host would.
struct Holder {
    child: Child,
    socket: std::path::PathBuf,
}

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn holder(folders: &Folders, idle_exit: u64) -> Holder {
    // this user's id, as the owner of a folder the test made
    let uid = std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(&folders.runtime).unwrap());
    let paths = Paths::with_runtime_dir(&folders.data, Some(&folders.runtime), uid);
    std::fs::create_dir_all(&paths.host_dir).unwrap();
    Registration {
        pid: std::process::id(),
        started_at: String::new(),
        boot_id: String::new(),
        version: String::new(),
        protocol: 1,
        features: vec![],
        socket: paths.socket(),
        secret: SECRET.to_string(),
        build: None,
    }
    .write(&paths.registration())
    .unwrap();
    let mut child = command(folders, &["hold", "--idle-exit", &idle_exit.to_string()])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let instance = line.trim().to_string();
    assert_eq!(instance.len(), 8, "{line:?}");
    Holder {
        child,
        socket: pinrail_host::holder::socket_path(&paths.socket_dir, &instance),
    }
}

/// A connection that has presented `secret`, and the holder's answer.
fn connect(holder: &Holder, secret: &str) -> (UnixStream, Value) {
    let stream = UnixStream::connect(&holder.socket).unwrap();
    fdpass::send(&stream, &json!({"secret": secret, "protocol": 1}), &[]).unwrap();
    let (answer, _) = fdpass::recv(&stream).unwrap().unwrap();
    (stream, answer)
}

fn request(stream: &UnixStream, value: Value, fds: &[i32]) -> (Value, Vec<OwnedFd>) {
    fdpass::send(stream, &value, fds).unwrap();
    loop {
        let (answer, fds) = fdpass::recv(stream).unwrap().unwrap();
        if answer.get("event").is_none() {
            return (answer, fds);
        }
    }
}

/// Reads from a PTY master until `wanted` appears.
fn read_until(master: &OwnedFd, wanted: &str) {
    let mut file = std::fs::File::from(master.try_clone().unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = Vec::new();
    let mut buf = [0u8; 1024];
    while !String::from_utf8_lossy(&seen).contains(wanted) {
        assert!(
            Instant::now() < deadline,
            "{:?}",
            String::from_utf8_lossy(&seen)
        );
        match file.read(&mut buf) {
            Ok(n) if n > 0 => seen.extend(&buf[..n]),
            _ => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn spawn_request(folders: &Folders, terminal: &str, script: &str) -> Value {
    json!({"op": "spawn", "id": 1, "terminal": terminal, "argv": ["sh", "-c", script],
           "cwd": folders.runtime, "env": {"PATH": "/usr/bin:/bin"}, "cols": 80, "rows": 24})
}

#[test]
fn a_program_outlives_the_host_that_started_it() {
    let folders = folders();
    let holder = holder(&folders, 30);
    let (first, hello) = connect(&holder, SECRET);
    assert_eq!(hello["protocol"], 1);
    assert!(hello["instance"].is_string());

    let ring = tempfile::tempfile().unwrap();
    let (spawned, fds) = request(
        &first,
        spawn_request(
            &folders,
            "t_1",
            "echo ready; read line; echo got:$line; read again; exit 7",
        ),
        &[std::os::fd::AsRawFd::as_raw_fd(&ring)],
    );
    assert!(spawned["pid"].as_u64().unwrap() > 0, "{spawned}");
    assert_eq!(fds.len(), 1);
    read_until(&fds[0], "ready");

    // the host goes away; the program does not
    drop(fds);
    drop(first);
    std::thread::sleep(Duration::from_millis(300));
    let (second, _) = connect(&holder, SECRET);
    let (listed, fds) = request(&second, json!({"op": "list", "id": 2}), &[]);
    assert_eq!(listed["terminals"][0]["terminal"], "t_1");
    assert_eq!(listed["terminals"][0]["ring"], true);
    assert!(listed["terminals"][0].get("exit").is_none());
    assert_eq!(fds.len(), 2, "the master and the ring");
    let mut master = std::fs::File::from(fds[0].try_clone().unwrap());
    master.write_all(b"hello\n").unwrap();
    read_until(&fds[0], "got:hello");

    // the exit is reported, and the program stays unreaped until forgotten;
    // the PTY is read meanwhile, as the host does, since a program's exit
    // waits for its terminal's output to be read on macOS
    master.write_all(b"bye\n").unwrap();
    let drain = fds[0].try_clone().unwrap();
    std::thread::spawn(move || {
        let mut drain = std::fs::File::from(drain);
        let mut buf = [0u8; 1024];
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if !matches!(drain.read(&mut buf), Ok(n) if n > 0) {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let event = loop {
        let (message, _) = fdpass::recv(&second).unwrap().unwrap();
        if message["event"] == "exited" {
            break message;
        }
        assert!(Instant::now() < deadline);
    };
    assert_eq!(event["terminal"], "t_1");
    assert_eq!(event["code"], 7);
    let pid = listed["terminals"][0]["pid"].as_u64().unwrap() as u32;
    // a pid exists until its process is reaped, as a zombie meanwhile
    let exists = |pid: u32| {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    };
    assert!(exists(pid), "reaped before forget");
    let (forgot, _) = request(
        &second,
        json!({"op": "forget", "id": 3, "terminal": "t_1"}),
        &[],
    );
    assert_eq!(forgot["ok"], true);
    let (listed, _) = request(&second, json!({"op": "list", "id": 4}), &[]);
    assert_eq!(listed["terminals"], json!([]));
    assert!(!exists(pid), "not reaped on forget");
}

#[test]
fn an_exit_while_no_host_is_connected_is_reported_to_the_next() {
    let folders = folders();
    let holder = holder(&folders, 30);
    let (first, _) = connect(&holder, SECRET);
    let (_, fds) = request(
        &first,
        spawn_request(&folders, "t_1", "sleep 0.3; exit 3"),
        &[],
    );
    drop(fds);
    drop(first);
    std::thread::sleep(Duration::from_secs(1));
    let (second, _) = connect(&holder, SECRET);
    let (event, _) = fdpass::recv(&second).unwrap().unwrap();
    assert_eq!(
        event,
        json!({"event": "exited", "terminal": "t_1", "code": 3})
    );
    let (listed, _) = request(&second, json!({"op": "list", "id": 2}), &[]);
    assert_eq!(listed["terminals"][0]["exit"], json!({"code": 3}));
}

#[test]
fn a_process_without_the_secret_is_refused() {
    let folders = folders();
    let holder = holder(&folders, 30);
    let (stream, answer) = connect(&holder, "wrong");
    assert_eq!(answer["error"], "forbidden");
    assert!(fdpass::recv(&stream).unwrap().is_none());
}

#[test]
fn a_spawn_that_cannot_happen_says_why() {
    let folders = folders();
    let holder = holder(&folders, 30);
    let (stream, _) = connect(&holder, SECRET);
    let mut missing = spawn_request(&folders, "t_1", "true");
    missing["cwd"] = json!("/no/such/folder");
    assert_eq!(request(&stream, missing, &[]).0["error"], "folder_missing");
    let mut unknown = spawn_request(&folders, "t_2", "true");
    unknown["argv"] = json!(["no-such-program-anywhere"]);
    assert_eq!(request(&stream, unknown, &[]).0["error"], "not_found");
}

#[test]
fn a_holder_with_nothing_to_hold_exits() {
    let folders = folders();
    let mut holder = holder(&folders, 1);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = holder.child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "the holder kept running");
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!holder.socket.exists());
}
