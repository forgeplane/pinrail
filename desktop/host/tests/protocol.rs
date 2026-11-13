//! The connection itself: every method of the schema is served, calls are
//! answered as they finish rather than in turn, channel 0 stays bounded, an
//! idle connection hears pings, and `connect` bridges a remote core in.
#![cfg(unix)]

mod common;

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::process::Stdio;
use std::time::{Duration, Instant};

use pinrail_host::client::{CallError, Client};
use pinrail_host::frame::{self, Frame, kind};
use pinrail_host::protocol;
use serde_json::{Value, json};

use common::{folders, serve};

/// A raw connection that has said hello.
fn raw(host: &common::Host) -> UnixStream {
    let mut stream = UnixStream::connect(&host.registration.socket).unwrap();
    let hello = json!({"jsonrpc": "2.0", "id": 0, "method": "hello",
                       "params": {"secret": host.registration.secret}});
    frame::write_blocking(&mut stream, &Frame::json(&hello)).unwrap();
    let reply = frame::read_blocking(&mut stream).unwrap().unwrap();
    let reply: Value = serde_json::from_slice(&reply.payload).unwrap();
    assert!(reply["result"].is_object(), "{reply}");
    stream
}

fn call(stream: &mut UnixStream, id: u64, method: &str, params: Value) {
    let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    frame::write_blocking(stream, &Frame::json(&message)).unwrap();
}

fn message(stream: &mut UnixStream) -> Option<Value> {
    let frame = frame::read_blocking(stream).ok()??;
    if frame.channel != 0 || frame.kind != kind::DATA {
        return message(stream);
    }
    serde_json::from_slice(&frame.payload).ok()
}

#[test]
fn every_method_of_the_schema_is_served() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut client = Client::connect(&host.registration).unwrap();
    // host.stop last, as it stops the host
    let mut methods: Vec<&str> = protocol::METHODS.iter().map(|m| m.name).collect();
    methods.sort_by_key(|m| *m == "host.stop");
    for method in methods {
        match client.call(method, json!({})) {
            Ok(_) => {}
            Err(CallError::Rpc(error)) => {
                let code = error["data"]["code"].as_str().unwrap();
                assert_ne!(code, "unsupported", "{method}");
                assert!(protocol::ERRORS.contains(&code), "{method}: {code}");
            }
            Err(other) => panic!("{method}: {other}"),
        }
    }
}

#[test]
fn a_slow_call_does_not_hold_up_the_others() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut stream = raw(&host);
    // a program that keeps drawing, so a message waits for it to go quiet
    call(
        &mut stream,
        1,
        "terminal.spawn",
        json!({"argv": ["sh", "-c", "while :; do echo drawing; sleep 0.05; done"],
               "cwd": folders.runtime, "cols": 80, "rows": 24}),
    );
    let spawned = message(&mut stream).unwrap();
    let id = spawned["result"]["terminal"].as_str().unwrap().to_string();
    call(
        &mut stream,
        2,
        "terminal.submit",
        json!({"terminal": id, "text": "x", "startup_quiet_ms": 5000}),
    );
    call(&mut stream, 3, "host.info", json!({}));
    let first = message(&mut stream).unwrap();
    assert_eq!(first["id"], 3, "{first}");
    call(&mut stream, 4, "terminal.stop", json!({"terminal": id}));
}

#[test]
fn an_idle_connection_hears_pings() {
    let folders = folders();
    let host = serve(&folders, &["--ping-every", "1"]);
    let mut stream = raw(&host);
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let ping = message(&mut stream).unwrap();
    assert_eq!(ping, json!({"jsonrpc": "2.0", "method": "ping"}));
}

#[test]
fn a_subscriber_that_falls_behind_is_told_it_missed_events() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut stream = raw(&host);
    call(&mut stream, 1, "events.subscribe", json!({}));
    assert_eq!(message(&mut stream).unwrap()["id"], 1);
    // far more than 8 MiB of title events, which the connection does not
    // read while they are written
    call(
        &mut stream,
        2,
        "terminal.spawn",
        json!({"argv": ["awk", "BEGIN { for (i = 0; i < 150000; i++) printf \"\\033]0;title %d\\007\", i }"],
               "cwd": folders.runtime, "cols": 80, "rows": 24}),
    );
    std::thread::sleep(Duration::from_secs(4));
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut titles = 0;
    let mut missed = false;
    let mut after_missed = 0;
    while let Some(message) = message(&mut stream) {
        match message["method"].as_str() {
            Some("title") if missed => after_missed += 1,
            Some("title") => titles += 1,
            Some("missed") => missed = true,
            _ => {}
        }
    }
    assert!(missed, "after {titles} titles");
    assert!(titles > 1_000);
    // nothing more until it subscribes again
    assert_eq!(after_missed, 0);
}

#[test]
fn a_connection_that_reads_no_replies_is_closed() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut stream = raw(&host);
    call(
        &mut stream,
        1,
        "terminal.spawn",
        json!({"argv": ["sh", "-c", "i=0; while [ $i -lt 3000 ]; do echo \"a long line of output number $i to fill the history\"; i=$((i+1)); done"],
               "cwd": folders.runtime, "cols": 80, "rows": 24}),
    );
    let id = message(&mut stream).unwrap()["result"]["terminal"]
        .as_str()
        .unwrap()
        .to_string();
    std::thread::sleep(Duration::from_millis(500));
    // about 160 KB a reply: 800 of them is far past 64 MiB
    let started = Instant::now();
    for n in 0..800 {
        let message = json!({"jsonrpc": "2.0", "id": n, "method": "terminal.text",
                             "params": {"terminal": id, "lines": 3000}});
        if frame::write_blocking(&mut stream, &Frame::json(&message)).is_err() {
            break;
        }
    }
    // while nothing is read, the host closes the connection rather than
    // keep the replies
    std::thread::sleep(Duration::from_secs(5));
    let mut buf = vec![0u8; 1 << 20];
    // macOS refuses the option on a socket the other side reset
    let reset = stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .is_err();
    let closed = reset
        || loop {
            match std::io::Read::read(&mut stream, &mut buf) {
                Ok(0) => break true,
                Ok(_) => {}
                // a reset closes it too; a timeout does not
                Err(e) => {
                    break !matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    );
                }
            }
        };
    assert!(closed, "after {:?}", started.elapsed());
    // and goes on serving the next
    let mut client = Client::connect(&host.registration).unwrap();
    assert!(client.call("host.info", json!({})).is_ok());
}

#[test]
fn connect_brings_a_remote_core_in_with_the_host_s_secret() {
    let folders = folders();
    let host = serve(&folders, &[]);
    let mut bridge = common::command(&folders, &["connect"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = bridge.stdin.take().unwrap();
    let mut stdout = bridge.stdout.take().unwrap();
    // the remote core does not know the secret
    let hello = json!({"jsonrpc": "2.0", "id": 1, "method": "hello",
                       "params": {"secret": "", "client": "pinrail-core/remote"}});
    frame::write_blocking(&mut stdin, &Frame::json(&hello)).unwrap();
    let reply: Value =
        serde_json::from_slice(&frame::read_blocking(&mut stdout).unwrap().unwrap().payload)
            .unwrap();
    assert_eq!(
        reply["result"]["boot_id"],
        host.registration.boot_id.as_str()
    );
    let info = json!({"jsonrpc": "2.0", "id": 2, "method": "host.info"});
    frame::write_blocking(&mut stdin, &Frame::json(&info)).unwrap();
    let reply: Value =
        serde_json::from_slice(&frame::read_blocking(&mut stdout).unwrap().unwrap().payload)
            .unwrap();
    assert_eq!(reply["result"]["terminals"], 0);
    stdin.flush().unwrap();
    drop(stdin);
    assert!(bridge.wait().unwrap().success());
}
