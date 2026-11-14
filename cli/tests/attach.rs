//! `pinrail attach` in a pseudo-terminal, against a mock core that answers
//! the run and ticket requests and serves the attach WebSocket: the
//! snapshot and the output are drawn, credit is returned, keys go to the
//! agent, and Ctrl-\ then d detaches without reaching it.
#![cfg(unix)]
#![allow(unsafe_code)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::{FromRawFd, OwnedFd};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::Message;

/// What the mock core's socket heard from the CLI.
#[derive(Debug, Default)]
struct Heard {
    input: Vec<u8>,
    credit: u64,
    texts: Vec<Value>,
    query: String,
}

/// Serves one attach: the HTTP requests before it, then the socket.
fn mock_core(listener: TcpListener, heard: mpsc::Sender<Heard>) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { break };
        let mut peek = [0u8; 512];
        let n = stream.peek(&mut peek).unwrap_or(0);
        let head = String::from_utf8_lossy(&peek[..n]).to_string();
        if head.contains("/attach?") {
            let query = head
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_string();
            heard.send(serve_socket(stream, query)).unwrap();
            return;
        }
        let mut request = Vec::new();
        let mut buf = [0u8; 4096];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut buf).unwrap();
            request.extend(&buf[..n]);
        }
        let line = String::from_utf8_lossy(&request)
            .lines()
            .next()
            .unwrap()
            .to_string();
        let body = if line.starts_with("GET /api/v1/runs/run_1") {
            json!({"id": "run_1", "state": "running", "terminal": "local:t_1"})
        } else if line.starts_with("POST /api/v1/terminals/local%3At_1/tickets") {
            json!({"ticket": "tk", "expires_in": 30})
        } else {
            json!({"error": "not_found", "message": line, "violations": []})
        }
        .to_string();
        let status = if body.contains("not_found") { 404 } else { 200 };
        write!(
            stream,
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    }
}

fn serve_socket(stream: TcpStream, query: String) -> Heard {
    let mut socket = tungstenite::accept(stream).unwrap();
    let text = |value: Value| Message::Text(value.to_string().into());
    socket
        .send(text(json!({"t": "hello", "v": 1, "terminal": "local:t_1", "cols": 80, "rows": 24, "role": "observer"})))
        .unwrap();
    socket
        .send(text(
            json!({"t": "snapshot_begin", "format": "vt", "cut": 0, "cols": 80, "rows": 24}),
        ))
        .unwrap();
    socket
        .send(Message::Binary(b"\x02SCREEN".to_vec().into()))
        .unwrap();
    socket.send(text(json!({"t": "snapshot_end"}))).unwrap();
    let mut output = vec![0x01];
    output.extend(0u64.to_le_bytes());
    output.extend(b" then output");
    socket.send(Message::Binary(output.into())).unwrap();
    let mut heard = Heard {
        query,
        ..Heard::default()
    };
    stream_timeout(&socket);
    loop {
        match socket.read() {
            Ok(Message::Binary(bytes)) if bytes.first() == Some(&0x10) => {
                heard.input.extend(&bytes[1..]);
            }
            Ok(Message::Text(message)) => {
                let value: Value = serde_json::from_str(&message).unwrap();
                if value["t"] == "credit" {
                    heard.credit += value["bytes"].as_u64().unwrap();
                }
                heard.texts.push(value);
            }
            Ok(Message::Close(_)) | Err(_) => return heard,
            Ok(_) => {}
        }
    }
}

fn stream_timeout(socket: &tungstenite::WebSocket<TcpStream>) {
    socket
        .get_ref()
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
}

/// A pseudo-terminal: the side the test reads and writes, and the side
/// the CLI runs on.
fn pty() -> (std::fs::File, OwnedFd) {
    let (mut leader, mut follower) = (0, 0);
    // SAFETY: openpty writes two descriptors, which the test then owns.
    let ok = unsafe {
        libc::openpty(
            &mut leader,
            &mut follower,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(ok, 0);
    // SAFETY: both descriptors are new and owned by nothing else.
    unsafe {
        (
            std::fs::File::from_raw_fd(leader),
            OwnedFd::from_raw_fd(follower),
        )
    }
}

/// Reads what the CLI draws until it contains `wanted`.
fn read_until(terminal: &mut std::fs::File, seen: &mut Vec<u8>, wanted: &[u8]) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut buf = [0u8; 4096];
    while !seen.windows(wanted.len()).any(|w| w == wanted) {
        assert!(
            Instant::now() < deadline,
            "{:?} never came: {:?}",
            String::from_utf8_lossy(wanted),
            String::from_utf8_lossy(seen)
        );
        match terminal.read(&mut buf) {
            Ok(0) | Err(_) => std::thread::sleep(Duration::from_millis(10)),
            Ok(n) => seen.extend(&buf[..n]),
        }
    }
}

#[test]
fn attach_draws_the_terminal_sends_keys_and_detaches_on_ctrl_backslash_then_d() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, heard) = mpsc::channel();
    std::thread::spawn(move || mock_core(listener, tx));
    let data = std::env::temp_dir().join(format!("pinrail-attach-test-{}", std::process::id()));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("person-token"), "tok").unwrap();

    let (mut terminal, follower) = pty();
    let child = Command::new(env!("CARGO_BIN_EXE_pinrail"))
        .arg("attach")
        .arg("run_1")
        .env("PINRAIL_URL", &url)
        .env("PINRAIL_DATA_DIR", &data)
        .env_remove("PINRAIL_JSON")
        .stdin(Stdio::from(follower.try_clone().unwrap()))
        .stdout(Stdio::from(follower.try_clone().unwrap()))
        .stderr(Stdio::from(follower))
        .spawn()
        .unwrap();
    // SAFETY: the descriptor stays open while the test uses it.
    unsafe {
        let flags = libc::fcntl(std::os::fd::AsRawFd::as_raw_fd(&terminal), libc::F_GETFL);
        libc::fcntl(
            std::os::fd::AsRawFd::as_raw_fd(&terminal),
            libc::F_SETFL,
            flags | libc::O_NONBLOCK,
        );
    }

    let mut seen = Vec::new();
    // it asks this terminal for its colours, then draws the screen
    read_until(&mut terminal, &mut seen, b"\x1b[c");
    terminal
        .write_all(b"\x1b]11;rgb:1010/2020/3030\x1b\\\x1b[?62;22c")
        .unwrap();
    read_until(&mut terminal, &mut seen, b"SCREEN then output");
    terminal.write_all(b"ls\r").unwrap();
    std::thread::sleep(Duration::from_millis(300));
    terminal.write_all(b"\x1c").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    terminal.write_all(b"d").unwrap();
    read_until(&mut terminal, &mut seen, b"[detached from run_1");

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let heard = heard.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(heard.query.contains("ticket=tk"), "{}", heard.query);
    assert!(heard.query.contains("formats=vt"), "{}", heard.query);
    // the keys, without the chord
    assert_eq!(heard.input, b"ls\r");
    // credit for the snapshot and the output
    assert_eq!(heard.credit, ("SCREEN".len() + " then output".len()) as u64);
    let facts = heard.texts.iter().find(|t| t["t"] == "facts").unwrap();
    assert_eq!(facts["bg"], "#102030");
    // the person's terminal is reset on the way out
    assert!(seen.windows(8).any(|w| w == b"\x1b[?1004l"));
    let _ = std::fs::remove_dir_all(&data);
}
