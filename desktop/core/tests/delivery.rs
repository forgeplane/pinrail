//! Messages to an agent Pinrail runs: typed into its terminal when it is
//! idle, held while it works, confirmed when its prompt hook reports the
//! same text, and sent through the queue when it cannot be typed.
#![cfg(unix)]

mod common;

use std::time::{Duration, Instant};

use axum::http::StatusCode;
use pinrail_core::runtime;
use serde_json::{Value, json};

use common::{App, call};

/// A core linked to a host, whose runs find the stand-in `claude`.
async fn linked() -> App {
    let hosts = tempfile::tempdir().unwrap();
    let registration = common::host(hosts.path());
    let path = common::stand_in_path(hosts.path());
    let app = common::app_with(|config| {
        config.host_socket = Some((registration.socket.clone(), registration.secret.clone()));
        config.run_env.insert("PATH".into(), path);
    });
    std::mem::forget(hosts);
    runtime::watch(app.state.clone());
    app.state
        .settings()
        .change(&json!({"labs": {"runtime": true}}))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.state.runtime().status()["state"] != "ready" {
        assert!(Instant::now() < deadline, "the host did not connect");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    app
}

/// A file the stand-in agent writes, once it has something in it.
async fn file(path: std::path::PathBuf) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(text) = std::fs::read_to_string(&path)
            && !text.is_empty()
        {
            return text;
        }
        assert!(
            Instant::now() < deadline,
            "{} was not written",
            path.display()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Starts the stand-in `claude` in a folder of its own, listening for what
/// is typed, and returns its session and folder.
async fn hosted(app: &App, name: &str) -> (String, std::path::PathBuf) {
    let folder = app.dir.path().join(name);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("listen"), "").unwrap();
    let (status, started) = call(
        app,
        "POST",
        "/api/v1/runs",
        Some(json!({"agent": "claude-code", "folder": folder})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    file(folder.join("token")).await;
    let session = started["session"]["id"].as_str().unwrap().to_string();
    (session, folder)
}

/// What the session's hooks report, as `pinrail hook` sends it.
async fn report(app: &App, session: &str, event: &str, extra: Value) {
    let mut body = json!({"event": event, "at": chrono::Utc::now().to_rfc3339()});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let (status, answer) = call(
        app,
        "POST",
        &format!("/api/v1/sessions/claude-code/{session}/reports"),
        Some(body),
    )
    .await;
    assert!(status.is_success(), "{status} {answer}");
}

async fn send(app: &App, session: &str, text: &str) -> Value {
    let (status, sent) = call(
        app,
        "POST",
        &format!("/api/v1/sessions/claude-code/{session}/messages"),
        Some(json!({ "text": text })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{sent}");
    sent
}

/// The session's message `id` once `done` holds for it.
async fn message_becomes(
    app: &App,
    session: &str,
    id: &Value,
    done: impl Fn(&Value) -> bool,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let (_, thread) = call(
            app,
            "GET",
            &format!("/api/v1/sessions/claude-code/{session}"),
            None,
        )
        .await;
        let message = thread["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| &m["id"] == id)
            .cloned()
            .unwrap();
        if done(&message) {
            return message;
        }
        assert!(Instant::now() < deadline, "{message}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_to_an_idle_agent_is_typed_and_confirmed_by_its_prompt() {
    let app = linked().await;
    let (session, folder) = hosted(&app, "app").await;
    report(&app, &session, "started", json!({})).await;

    let sent = send(&app, &session, "Use the v2 endpoint.").await;
    assert_eq!(sent["route"], "type");
    let heard = file(folder.join("heard")).await;
    assert!(heard.contains("Use the v2 endpoint."), "{heard:?}");
    let typing = message_becomes(&app, &session, &sent["id"], |m| m["state"] == "typing").await;
    assert_eq!(typing["delivered_at"], Value::Null);

    // the prompt hook reports it, with the person's own words added after
    report(
        &app,
        &session,
        "prompt",
        json!({"detail": {"prompt": "  Use the v2 endpoint.\r\nand hurry"}}),
    )
    .await;
    let delivered =
        message_becomes(&app, &session, &sent["id"], |m| m["state"] == "delivered").await;
    assert!(delivered["delivered_at"].is_string());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_waits_while_the_agent_works_and_is_typed_once_it_is_idle() {
    let app = linked().await;
    let (session, folder) = hosted(&app, "app").await;
    report(
        &app,
        &session,
        "prompt",
        json!({"detail": {"prompt": "fix the login"}}),
    )
    .await;

    let sent = send(&app, &session, "Also check the logout.").await;
    let held = message_becomes(&app, &session, &sent["id"], |m| m["held"] == "working").await;
    assert_eq!(held["state"], "queued");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !folder.join("heard").exists(),
        "typed while the agent worked"
    );

    report(&app, &session, "turn_ended", json!({"end": "done"})).await;
    let heard = file(folder.join("heard")).await;
    assert!(heard.contains("Also check the logout."), "{heard:?}");
    message_becomes(&app, &session, &sent["id"], |m| m["state"] == "typing").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unconfirmed_message_is_retried_only_by_the_person() {
    let app = linked().await;
    let (session, folder) = hosted(&app, "app").await;
    report(&app, &session, "started", json!({})).await;
    let sent = send(&app, &session, "Use the v2 endpoint.").await;
    message_becomes(&app, &session, &sent["id"], |m| m["state"] == "typing").await;

    // no confirmation within 10 seconds
    let db = rusqlite::Connection::open(app.state.config().db_path()).unwrap();
    db.execute(
        "UPDATE session_messages SET typed_at = ?1",
        [(chrono::Utc::now() - chrono::Duration::seconds(11))
            .format("%Y-%m-%dT%H:%M:%S%.6fZ")
            .to_string()],
    )
    .unwrap();
    app.state.runtime().sweep_messages().unwrap();
    message_becomes(&app, &session, &sent["id"], |m| m["state"] == "unconfirmed").await;

    file(folder.join("heard")).await;
    std::fs::remove_file(folder.join("heard")).unwrap();
    let (status, _) = call(
        &app,
        "POST",
        &format!(
            "/api/v1/sessions/claude-code/{session}/messages/{}/type",
            sent["id"]
        ),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let heard = file(folder.join("heard")).await;
    assert!(heard.contains("Use the v2 endpoint."), "{heard:?}");
    message_becomes(&app, &session, &sent["id"], |m| m["state"] == "typing").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_held_for_half_an_hour_goes_to_the_queue() {
    let app = linked().await;
    let (session, _) = hosted(&app, "app").await;
    report(
        &app,
        &session,
        "prompt",
        json!({"detail": {"prompt": "fix the login"}}),
    )
    .await;
    let sent = send(&app, &session, "Also check the logout.").await;
    message_becomes(&app, &session, &sent["id"], |m| m["held"] == "working").await;

    let db = rusqlite::Connection::open(app.state.config().db_path()).unwrap();
    db.execute(
        "UPDATE session_messages SET sent_at = ?1",
        [(chrono::Utc::now() - chrono::Duration::minutes(31))
            .format("%Y-%m-%dT%H:%M:%S%.6fZ")
            .to_string()],
    )
    .unwrap();
    app.state.runtime().sweep_messages().unwrap();
    let queued = message_becomes(&app, &session, &sent["id"], |m| m["route"] == "queue").await;
    assert_eq!(queued["state"], "queued");
    assert_eq!(queued["held"], Value::Null);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_queued_while_the_session_was_stopped_is_typed_once_it_is_resumed() {
    let app = linked().await;
    let folder = app.dir.path().join("app");
    std::fs::create_dir_all(&folder).unwrap();
    // a run that ends at once leaves a stopped session
    std::fs::write(folder.join("exit"), "0").unwrap();
    let (status, started) = call(
        &app,
        "POST",
        "/api/v1/runs",
        Some(json!({"agent": "claude-code", "folder": folder})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    let session = started["session"]["id"].as_str().unwrap().to_string();
    let run = started["run"]["id"].as_str().unwrap().to_string();
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.state.runtime().message_route("claude-code", &session) != "queue"
        || call(&app, "GET", &format!("/api/v1/runs/{run}"), None)
            .await
            .1["state"]
            != "ended"
    {
        assert!(Instant::now() < deadline, "the run did not end");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // written while nothing runs the session: it waits in the queue
    let sent = send(&app, &session, "Use the v2 endpoint.").await;
    assert_eq!(sent["route"], "queue");

    // resumed in Pinrail, the agent listens, says it started, and is idle
    std::fs::remove_file(folder.join("exit")).unwrap();
    std::fs::write(folder.join("listen"), "").unwrap();
    let (status, resumed) = call(
        &app,
        "POST",
        &format!("/api/v1/sessions/claude-code/{session}/resume"),
        Some(json!({"cols": 80, "rows": 24})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{resumed}");
    report(&app, &session, "started", json!({})).await;
    let heard = file(folder.join("heard")).await;
    assert!(heard.contains("Use the v2 endpoint."), "{heard:?}");
    let typing = message_becomes(&app, &session, &sent["id"], |m| m["state"] == "typing").await;
    assert_eq!(typing["route"], "type");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_pinrail_does_not_run_gets_its_messages_by_the_queue() {
    let app = linked().await;
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-outside/status",
        Some(json!({"text": "Working"})),
    )
    .await;
    assert!(status.is_success());
    let sent = send(&app, "s-outside", "Use the v2 endpoint.").await;
    assert_eq!(sent["route"], "queue");
    assert_eq!(sent["state"], "queued");

    // the session's reports leave it there, and announce no change of it
    let mut notices = app.state.events().subscribe();
    report(&app, "s-outside", "started", json!({})).await;
    report(&app, "s-outside", "turn_ended", json!({"end": "done"})).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    while let Ok(notice) = notices.try_recv() {
        assert_ne!(notice.kind, "session_message_state", "{notice:?}");
    }
    let still = message_becomes(&app, "s-outside", &sent["id"], |_| true).await;
    assert_eq!(
        (still["state"].as_str(), still["route"].as_str()),
        (Some("queued"), Some("queue"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn each_change_of_a_message_s_state_is_announced() {
    let app = linked().await;
    let (session, _) = hosted(&app, "app").await;
    report(&app, &session, "started", json!({})).await;
    let mut notices = app.state.events().subscribe();
    let sent = send(&app, &session, "Use the v2 endpoint.").await;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let notice = tokio::time::timeout_at(deadline.into(), notices.recv())
            .await
            .expect("no session_message_state notice")
            .unwrap();
        if notice.kind == "session_message_state" {
            let message = &notice.session.unwrap()["message"];
            assert_eq!(message["id"], sent["id"]);
            assert_eq!(message["state"], "typing");
            break;
        }
    }
}
