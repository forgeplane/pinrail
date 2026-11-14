//! The core links to the terminal host while `/labs/runtime` is on, and
//! lets it be when it is turned off: the host and its agents keep running.
#![cfg(unix)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use pinrail_core::{Config, Pinrail, runtime};
use pinrail_host::client::Client;
use pinrail_host::registration::Registration;
use serde_json::{Value, json};

/// A host served from a thread of the test, and its registration.
fn host(dir: &std::path::Path) -> Registration {
    let data_dir = dir.join("host-data");
    let config = pinrail_host::server::Config {
        data_dir: data_dir.clone(),
        idle_exit: Duration::from_secs(600),
        ping_every: Duration::from_secs(15),
    };
    std::thread::spawn(move || pinrail_host::server::serve(config));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(registration) = Registration::read(&data_dir.join("host/host.json")) {
            return registration;
        }
        assert!(Instant::now() < deadline, "the host did not start");
        std::thread::sleep(Duration::from_millis(20));
    }
}

async fn status_becomes(app: &Pinrail, state: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = app.runtime().status();
        if status["state"] == state {
            return status;
        }
        assert!(Instant::now() < deadline, "{status}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_core_links_to_the_host_while_the_runtime_is_on() {
    let dir = tempfile::tempdir().unwrap();
    let registration = host(dir.path());
    let mut config = Config::new(dir.path().join("data"), 0);
    config.host_socket = Some((registration.socket.clone(), registration.secret.clone()));
    let app = Arc::new(Pinrail::open(config).unwrap());
    runtime::watch(app.clone());
    // off by default
    assert_eq!(app.runtime().status(), json!({"enabled": false}));

    app.settings()
        .change(&json!({"labs": {"runtime": true}}))
        .unwrap();
    let status = status_becomes(&app, "ready").await;
    assert!(status["instance"].is_string());
    let link = app.runtime().host().unwrap();
    assert_eq!(
        link.call("host.info", json!({})).await.unwrap()["terminals"],
        0
    );

    // turning it off disconnects, and the host keeps running
    app.settings()
        .change(&json!({"labs": {"runtime": false}}))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.runtime().status() != json!({"enabled": false}) {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let still = tokio::task::spawn_blocking(move || Client::connect(&registration).is_ok())
        .await
        .unwrap();
    assert!(still);
}
