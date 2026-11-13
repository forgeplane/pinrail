//! `pinrail-host serve`, `status` and `stop`, run as the core and a person
//! run them: the registration, the lock, the `hello`, one controller at a
//! time, and the ways the host stops.
#![cfg(unix)]

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use pinrail_host::client::{CallError, Client};
use pinrail_host::frame::{self, Frame, kind};
use pinrail_host::registration::Registration;
use serde_json::json;

/// A data folder and a runtime folder of the test's own.
struct Folders {
    _root: tempfile::TempDir,
    data: PathBuf,
    runtime: PathBuf,
}

fn folders() -> Folders {
    // short, so that the socket path fits
    let root = tempfile::Builder::new()
        .prefix("ph")
        .tempdir_in("/tmp")
        .unwrap();
    let data = root.path().join("data");
    let runtime = root.path().join("run");
    std::fs::create_dir(&runtime).unwrap();
    Folders {
        data,
        runtime,
        _root: root,
    }
}

fn command(folders: &Folders, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pinrail-host"));
    command
        .args(args)
        .arg("--data-dir")
        .arg(&folders.data)
        .env("TMPDIR", &folders.runtime)
        .env("XDG_RUNTIME_DIR", &folders.runtime)
        .stdin(Stdio::null());
    command
}

fn run(folders: &Folders, args: &[&str]) -> Output {
    command(folders, args).output().unwrap()
}

/// A running `serve`, killed when dropped.
struct Host {
    child: Child,
    registration: Registration,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn registration_path(folders: &Folders) -> PathBuf {
    folders.data.join("host/host.json")
}

fn serve(folders: &Folders, extra: &[&str]) -> Host {
    let mut args = vec!["serve"];
    args.extend(extra);
    let mut child = command(folders, &args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let path = registration_path(folders);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(registration) = Registration::read(&path)
            && registration.pid == child.id()
        {
            return Host {
                child,
                registration,
            };
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("the host exited with {status}");
        }
        assert!(Instant::now() < deadline, "the host did not register");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Waits for the process to exit and returns whether it exited cleanly.
fn exits_within(host: &mut Host, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if let Some(status) = host.child.try_wait().unwrap() {
            return status.success();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn the_host_registers_itself_and_answers_a_controller() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let registration = &host.registration;
    assert_eq!(registration.boot_id, pinrail_host::boot_id().unwrap());
    assert_eq!(registration.secret.len(), 64);
    // the socket's folder is in the runtime folder and closed to others
    assert!(registration.socket.starts_with(&folders.runtime));
    assert!(registration.socket.ends_with("host.sock"));
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(registration.socket.parent().unwrap()), 0o700);
    assert_eq!(mode(&registration.socket), 0o600);
    assert_eq!(mode(&registration_path(&folders)), 0o600);

    let mut client = Client::connect(registration).unwrap();
    assert_eq!(client.hello["generation"], 1);
    assert_eq!(client.hello["boot_id"], registration.boot_id.as_str());
    assert_eq!(client.hello["instance"].as_str().unwrap().len(), 16);
    let info = client.call("host.info", json!({})).unwrap();
    assert_eq!(info["terminals"], 0);
    assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
    let unknown = client.call("terminal.dance", json!({})).unwrap_err();
    assert!(matches!(unknown, CallError::Rpc(e) if e["data"]["code"] == "unknown_method"));
}

#[test]
fn a_connection_without_the_secret_is_closed() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let wrong = Registration {
        secret: "0".repeat(64),
        ..host.registration.clone()
    };
    assert!(matches!(
        Client::connect(&wrong),
        Err(CallError::Closed(None))
    ));
    // so is one whose first frame is not hello
    let mut stream = UnixStream::connect(&host.registration.socket).unwrap();
    let info = json!({"jsonrpc": "2.0", "id": 1, "method": "host.info"});
    frame::write_blocking(&mut stream, &Frame::json(&info)).unwrap();
    assert_eq!(frame::read_blocking(&mut stream).unwrap(), None);
}

#[test]
fn a_frame_over_the_limit_closes_the_connection() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut stream = UnixStream::connect(&host.registration.socket).unwrap();
    let mut header = vec![0, 0, 0, 0, kind::DATA];
    header.extend(((frame::MAX_PAYLOAD + 1) as u32).to_le_bytes());
    stream.write_all(&header).unwrap();
    assert_eq!(frame::read_blocking(&mut stream).unwrap(), None);
}

#[test]
fn a_second_controller_replaces_the_first() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let first = Client::connect(&host.registration).unwrap();
    let mut second = Client::connect(&host.registration).unwrap();
    assert_eq!(first.wait_closed(), Some(json!({"reason": "replaced"})));
    assert!(second.call("host.info", json!({})).is_ok());
}

#[test]
fn only_one_host_runs_for_a_data_folder() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let output = run(&folders, &["serve"]);
    assert!(!output.status.success());
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(
        message.contains(&format!(
            "another host is running (process {})",
            host.child.id()
        )),
        "{message}"
    );
    // the running host's registration is untouched
    assert_eq!(
        Registration::read(&registration_path(&folders)),
        Some(host.registration.clone())
    );
}

#[test]
fn status_prints_the_registration_without_the_secret() {
    let folders = folders();
    let output = run(&folders, &["status"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        r#"{"running":false}"#
    );

    let host = serve(&folders, &[]);
    let output = run(&folders, &["status"]);
    assert!(output.status.success());
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["running"], true);
    assert_eq!(status["pid"], host.child.id());
    assert_eq!(status["terminals"], 0);
    assert!(status.get("secret").is_none());
}

#[test]
fn stop_ends_the_host_and_removes_its_registration_and_socket() {
    let folders = folders();
    let mut host = serve(&folders, &[]);
    let output = run(&folders, &["stop"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(exits_within(&mut host, Duration::from_secs(5)));
    assert!(!registration_path(&folders).exists());
    assert!(!host.registration.socket.exists());
    let log = std::fs::read_to_string(folders.data.join("host/host.log")).unwrap();
    assert!(log.contains("stopped: host.stop"), "{log}");
    // and the next host can start
    let _next = serve(&folders, &[]);
}

#[test]
fn a_host_with_no_controller_exits_when_idle() {
    let folders = folders();
    let mut host = serve(&folders, &["--idle-exit", "1"]);
    assert!(exits_within(&mut host, Duration::from_secs(5)));
    assert!(!registration_path(&folders).exists());
}

#[test]
fn a_controller_keeps_the_host_from_idling() {
    let folders = folders();
    let mut host = serve(&folders, &["--idle-exit", "1"]);
    let mut client = Client::connect(&host.registration).unwrap();
    assert!(!exits_within(&mut host, Duration::from_secs(2)));
    assert!(client.call("host.info", json!({})).is_ok());
    drop(client);
    assert!(exits_within(&mut host, Duration::from_secs(5)));
}

#[test]
fn a_registration_from_another_boot_is_not_adopted() {
    let folders = folders();
    let host = serve(&folders, &[]);
    // the same live host, recorded as if before a restart
    let stale = Registration {
        boot_id: "a-boot-before-this-one".to_string(),
        ..host.registration.clone()
    };
    stale.write(&registration_path(&folders)).unwrap();
    let output = run(&folders, &["status"]);
    assert_eq!(output.status.code(), Some(1));
}
