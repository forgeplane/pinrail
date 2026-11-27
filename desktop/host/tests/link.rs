//! The core's link to the host: it starts or adopts the host, reconnects
//! when the connection drops, starts a new host when the host is gone, and
//! resumes the events of the same host where it left off.

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
        match tokio::time::timeout(common::within(20), events.recv())
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
        match tokio::time::timeout(common::within(10), events.recv())
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
            common::kill(registration.pid);
        }
        // and the holders, which would start a host again for the programs
        // they keep
        common::kill_holders(&self.0.join("host/holders"));
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
    common::kill(pid);
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
    // a holder runs before the other controller comes: its spawn then
    // takes far less than the second the link waits before it reconnects,
    // whatever starting a holder takes, such as the first-launch check
    link.call(
        "terminal.spawn",
        json!({"argv": common::sleep(30), "cwd": "/", "cols": 80, "rows": 24}),
    )
    .await
    .unwrap();

    // another controller takes the host for a while and starts a terminal
    let registration = registration(&folders);
    let blocking = tokio::task::spawn_blocking(move || {
        let mut other = Client::connect(&registration).unwrap();
        other
            .call(
                "terminal.spawn",
                json!({"argv": common::shell("exit 3"), "cwd": "/", "cols": 80, "rows": 24}),
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
            json!({"argv": common::shell("echo hello there; sleep 30"), "cwd": "/",
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
    let first = tokio::time::timeout(common::within(5), channel.frames.recv())
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

/// A host that answers `hello` and the subscription, then closes the
/// connection, `times` times.
async fn host_that_drops(mut listener: pinrail_host::net::AsyncListener, times: usize) {
    use pinrail_host::frame::{self, Frame};
    for _ in 0..times {
        let Ok(stream) = listener.accept().await else {
            return;
        };
        let (mut reader, mut writer) = pinrail_host::net::split(stream);
        let _hello = frame::read(&mut reader).await;
        let hello = json!({"jsonrpc": "2.0", "id": 0,
                           "result": {"generation": pinrail_host::GENERATION, "instance": "i1"}});
        frame::write(&mut writer, &Frame::json(&hello))
            .await
            .unwrap();
        let Ok(Some(subscribe)) = frame::read(&mut reader).await else {
            return;
        };
        let subscribe: Value = serde_json::from_slice(&subscribe.payload).unwrap();
        let answer = json!({"jsonrpc": "2.0", "id": subscribe["id"], "result": {"terminals": []}});
        frame::write(&mut writer, &Frame::json(&answer))
            .await
            .unwrap();
        // ready for a moment, then gone
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tokio::test]
async fn each_reconnect_after_a_ready_connection_starts_the_backoff_again() {
    #[cfg(unix)]
    let dir = tempfile::Builder::new()
        .prefix("pl")
        .tempdir_in("/tmp")
        .unwrap();
    #[cfg(unix)]
    let socket = dir.path().join("host.sock");
    // a pipe needs no folder, only a name of the test's own
    #[cfg(windows)]
    let socket = PathBuf::from(format!(
        r"\\.\pipe\pinrail-link-test-{}",
        pinrail_host::random_hex(4)
    ));
    let listener = pinrail_host::net::AsyncListener::bind(&socket).unwrap();
    tokio::spawn(host_that_drops(listener, 3));
    let link = HostLink::start(
        Connector::Socket {
            socket,
            secret: "secret".to_string(),
        },
        "test",
    );
    let mut states = link.watch();
    let mut attempts = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while attempts.len() < 3 {
        tokio::select! {
            changed = states.changed() => changed.unwrap(),
            () = tokio::time::sleep_until(deadline) => panic!("{attempts:?}"),
        }
        let state = states.borrow().clone();
        if let HostState::Reconnecting { attempt, .. } = state {
            attempts.push(attempt);
        }
    }
    link.stop();
    // every connection became ready, so each retry starts from the first delay
    assert_eq!(attempts, [1, 1, 1]);
}
