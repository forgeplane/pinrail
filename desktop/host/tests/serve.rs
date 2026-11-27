//! `pinrail-host serve`, `status` and `stop`, run as the core and a person
//! run them: the registration, the lock, the `hello`, one controller at a
//! time, and the ways the host stops.

mod common;

use std::io::Write;
use std::time::Duration;

use common::{exits_within, folders, registration_path, run, serve};

use pinrail_host::client::{CallError, Client};
use pinrail_host::frame::{self, Frame, kind};
use pinrail_host::net;
use pinrail_host::registration::Registration;
use serde_json::json;

#[test]
fn the_host_registers_itself_and_answers_a_controller() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let registration = &host.registration;
    assert_eq!(registration.boot_id, pinrail_host::boot_id().unwrap());
    assert_eq!(registration.secret.len(), 64);
    // the socket's folder is in the runtime folder and closed to others
    #[cfg(unix)]
    {
        assert!(registration.socket.starts_with(&folders.runtime));
        assert!(registration.socket.ends_with("host.sock"));
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(registration.socket.parent().unwrap()), 0o700);
        assert_eq!(mode(&registration.socket), 0o600);
        assert_eq!(mode(&registration_path(&folders)), 0o600);
    }
    // a pipe named for the data folder; that only this user opens it, and
    // the registration, is tested with `net` and `paths`
    #[cfg(windows)]
    {
        let pipe = registration.socket.to_string_lossy();
        assert!(pipe.starts_with(r"\\.\pipe\pinrail-host-"), "{pipe}");
        assert!(pipe.ends_with(".host.sock"), "{pipe}");
        assert!(registration_path(&folders).is_file());
    }

    let mut client = Client::connect(registration).unwrap();
    assert_eq!(client.hello["generation"], 1);
    assert_eq!(client.hello["boot_id"], registration.boot_id.as_str());
    assert_eq!(client.hello["instance"].as_str().unwrap().len(), 16);
    let info = client.call("host.info", json!({})).unwrap();
    assert_eq!(info["terminals"], 0);
    assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
    let unknown = client.call("terminal.dance", json!({})).unwrap_err();
    assert!(matches!(unknown, CallError::Rpc(e) if e["data"]["code"] == "unsupported"));
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
    let mut stream = net::connect(&host.registration.socket).unwrap();
    let info = json!({"jsonrpc": "2.0", "id": 1, "method": "host.info"});
    frame::write_blocking(&mut stream, &Frame::json(&info)).unwrap();
    assert_eq!(frame::read_blocking(&mut stream).unwrap(), None);
}

#[test]
fn a_frame_over_the_limit_closes_the_connection() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut stream = net::connect(&host.registration.socket).unwrap();
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
fn a_stop_is_kept_when_its_connection_closes_before_the_host_notes_it() {
    let folders = folders();
    let mut host = serve(&folders, &[]);
    let mut stream = net::connect(&host.registration.socket).unwrap();
    let hello = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "hello",
        "params": {
            "secret": host.registration.secret,
            "client": "test",
            "generation": pinrail_host::GENERATION,
            "features": [],
        },
    });
    frame::write_blocking(&mut stream, &Frame::json(&hello)).unwrap();
    assert!(frame::read_blocking(&mut stream).unwrap().is_some());
    // the controller asks the host to stop and closes its side at once,
    // so the host reads the end of the connection right after the call
    let stop = json!({"jsonrpc": "2.0", "id": 2, "method": "host.stop", "params": {}});
    frame::write_blocking(&mut stream, &Frame::json(&stop)).unwrap();
    #[cfg(unix)]
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    // a pipe has no half to close: the client lets go of all of it, and
    // what it wrote stays for the host to read
    #[cfg(windows)]
    drop(stream);
    assert!(exits_within(&mut host, Duration::from_secs(5)));
    let log = std::fs::read_to_string(folders.data.join("host/host.log")).unwrap();
    assert!(log.contains("stopped: host.stop"), "{log}");
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
