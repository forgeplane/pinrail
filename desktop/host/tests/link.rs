//! The core's link to the host: it starts or adopts the host, reconnects
//! when the connection drops, starts a new host when the host is gone, and
//! resumes the events of the same host where it left off.
#![cfg(unix)]

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use pinrail_host::client::Client;
use pinrail_host::frame::kind;
use pinrail_host::launch::{Launcher, Method};
use pinrail_host::link::{Connector, HostError, HostEvent, HostLink, HostState};
use pinrail_host::registration::Registration;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use common::{Folders, folders};

fn launcher(folders: &Folders) -> Connector {
    let mut env: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| !["TMPDIR", "XDG_RUNTIME_DIR", "SHELL", "HOME"].contains(&k.as_str()))
        .collect();
    for (k, v) in [
        ("TMPDIR", &folders.runtime),
        ("XDG_RUNTIME_DIR", &folders.runtime),
        ("HOME", &folders.home),
    ] {
        env.push((k.to_string(), v.to_string_lossy().into_owned()));
    }
    env.push(("SHELL".to_string(), "/bin/sh".to_string()));
    Connector::Launch(Arc::new(Launcher {
        bundled: PathBuf::from(env!("CARGO_BIN_EXE_pinrail-host")),
        data_dir: folders.data.clone(),
        env,
        methods: vec![Method::Detached],
    }))
}

/// The next `Connected` event.
async fn connected(events: &mut broadcast::Receiver<HostEvent>) -> (String, Value) {
    loop {
        match tokio::time::timeout(Duration::from_secs(20), events.recv())
            .await
            .expect("connected in time")
            .unwrap()
        {
            HostEvent::Connected {
                instance,
                terminals,
            } => return (instance, terminals),
            _ => continue,
        }
    }
}

/// The next host event called `method`.
async fn event(events: &mut broadcast::Receiver<HostEvent>, method: &str) -> Value {
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap_or_else(|_| panic!("no {method}"))
            .unwrap()
        {
            HostEvent::Event { method: m, params } if m == method => return params,
            _ => continue,
        }
    }
}

fn registration(folders: &Folders) -> Registration {
    Registration::read(&folders.data.join("host/host.json")).unwrap()
}

/// Stops the host the test started, whatever happened.
struct StopHost(PathBuf);

impl Drop for StopHost {
    fn drop(&mut self) {
        if let Some(registration) = Registration::read(&self.0.join("host/host.json")) {
            let _ = std::process::Command::new("kill")
                .arg("-9")
                .arg(registration.pid.to_string())
                .status();
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_link_starts_a_host_and_starts_a_new_one_when_it_is_gone() {
    let folders = folders();
    let _stop = StopHost(folders.data.clone());
    let link = HostLink::start(launcher(&folders), "pinrail-core/test");
    let mut events = link.events();
    let (first, terminals) = connected(&mut events).await;
    assert_eq!(terminals, json!([]));
    assert_eq!(
        link.state(),
        HostState::Ready {
            instance: first.clone()
        }
    );
    let info = link.call("host.info", json!({})).await.unwrap();
    assert_eq!(info["terminals"], 0);

    // the host dies: calls fail as offline, and a new host is started
    let pid = registration(&folders).pid;
    std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .unwrap();
    let (second, _) = connected(&mut events).await;
    assert_ne!(second, first);
    assert_ne!(registration(&folders).pid, pid);
    assert!(link.call("host.info", json!({})).await.is_ok());
    link.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_dropped_connection_the_link_gets_the_events_it_missed() {
    let folders = folders();
    let _stop = StopHost(folders.data.clone());
    let link = HostLink::start(launcher(&folders), "pinrail-core/test");
    let mut events = link.events();
    let (instance, _) = connected(&mut events).await;

    // another controller takes the host for a while and starts a terminal
    let registration = registration(&folders);
    let blocking = tokio::task::spawn_blocking(move || {
        let mut other = Client::connect(&registration).unwrap();
        other
            .call(
                "terminal.spawn",
                json!({"argv": ["sh", "-c", "exit 3"], "cwd": "/", "cols": 80, "rows": 24}),
            )
            .unwrap()
    });
    let spawned = blocking.await.unwrap();
    let id = spawned["terminal"].as_str().unwrap().to_string();

    // the link comes back to the same host and resumes its events
    let (again, terminals) = connected(&mut events).await;
    assert_eq!(again, instance);
    assert!(
        terminals
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["terminal"] == id.as_str())
    );
    let exited = event(&mut events, "exited").await;
    assert_eq!(exited["terminal"], id.as_str());
    link.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_channel_through_the_link_gets_its_frames() {
    let folders = folders();
    let _stop = StopHost(folders.data.clone());
    let link = HostLink::start(launcher(&folders), "pinrail-core/test");
    connected(&mut link.events()).await;
    let spawned = link
        .call(
            "terminal.spawn",
            json!({"argv": ["sh", "-c", "echo hello there; sleep 30"], "cwd": "/",
                   "cols": 80, "rows": 24}),
        )
        .await
        .unwrap();
    let terminal = spawned["terminal"].as_str().unwrap().to_string();
    let mut channel = link
        .attach(json!({"terminal": terminal, "cols": 80, "rows": 24, "formats": ["vt"]}))
        .await
        .unwrap();
    assert_eq!(channel.format, "vt");
    let first = tokio::time::timeout(Duration::from_secs(5), channel.frames.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.kind, kind::META);
    let begin: Value = serde_json::from_slice(&first.payload).unwrap();
    assert_eq!(begin["type"], "snapshot_begin");
    channel.send(kind::DATA, b"x".to_vec()).unwrap();
    link.call("terminal.stop", json!({"terminal": terminal}))
        .await
        .unwrap();
    link.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_that_cannot_reach_its_host_keeps_trying_and_calls_fail_as_offline() {
    let folders = folders();
    let link = HostLink::start(
        Connector::Socket {
            socket: folders.runtime.join("nothing.sock"),
            secret: "0".repeat(64),
        },
        "pinrail-core/test",
    );
    let mut state = link.watch();
    let reconnecting = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let HostState::Reconnecting { attempt, .. } = state.borrow_and_update().clone() {
                return attempt;
            }
            state.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(reconnecting >= 1);
    assert_eq!(
        link.call("host.info", json!({})).await,
        Err(HostError::Offline)
    );
    link.stop();
    tokio::time::timeout(Duration::from_secs(5), async {
        while link.state() != HostState::Offline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
