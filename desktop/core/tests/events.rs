//! A notice replayed to a client that reconnects is the one sent live.

use serde_json::{Value, json};

mod common;
use common::{App, app, call};

const REPO: &str = "forgeplane/pinrail";

/// A notice as a client receives it, without its review: a replay gives the
/// review as it is now, which may have changed since.
fn shape(notice: &pinrail_core::events::Notice) -> Value {
    let mut value = serde_json::to_value(notice).unwrap();
    value.as_object_mut().unwrap().remove("review");
    value
}

async fn agent(app: &App, method: &str, path: &str, body: Value) -> Value {
    let mut body = body;
    body["by"] = json!("claude-code");
    body["session"] = json!("s-1");
    let (code, answer) = call(app, method, path, Some(body)).await;
    assert!(code.is_success(), "{method} {path}: {code} {answer}");
    answer
}

async fn person(app: &App, method: &str, path: &str, body: Option<Value>) -> Value {
    let (code, answer) = call(app, method, path, body).await;
    assert!(code.is_success(), "{method} {path}: {code} {answer}");
    answer
}

#[tokio::test]
async fn every_replayed_notice_matches_the_live_one() {
    let app = app();
    let mut live = app.state.events().subscribe();
    let status = "/api/v1/sessions/claude-code/s-1/status";

    agent(&app, "POST", status, json!({"text": "Working"})).await;
    let sent = person(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/messages",
        Some(json!({"text": "Use the new endpoint."})),
    )
    .await;
    let deleted = format!("/api/v1/sessions/claude-code/s-1/messages/{}", sent["id"]);
    person(&app, "DELETE", &deleted, None).await;
    person(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/messages",
        Some(json!({"text": "Run the tests too."})),
    )
    .await;
    // delivers the message
    agent(&app, "POST", status, json!({"text": "Still working"})).await;
    let archive = "/api/v1/sessions/claude-code/s-1/archive";
    person(&app, "PUT", archive, None).await;
    person(&app, "DELETE", archive, None).await;
    agent(
        &app,
        "POST",
        "/api/v1/items",
        json!({"repo": REPO, "title": "Work"}),
    )
    .await;
    agent(
        &app,
        "PATCH",
        "/api/v1/items/PIN-1",
        json!({"claim": "claim"}),
    )
    .await;
    person(
        &app,
        "PATCH",
        "/api/v1/repos/forgeplane%2Fpinrail",
        Some(json!({"prefix": "PR"})),
    )
    .await;
    person(&app, "DELETE", "/api/v1/items/PR-1", None).await;
    person(
        &app,
        "PATCH",
        "/api/v1/settings",
        Some(json!({"appearance": {"theme": "dark"}})),
    )
    .await;

    let mut sent_live = Vec::new();
    while let Ok(notice) = live.try_recv() {
        sent_live.push(notice);
    }
    let replayed = app.state.events().after(0, 1000).unwrap();
    let kinds: Vec<&str> = sent_live.iter().map(|n| n.kind.as_str()).collect();
    for kind in [
        "session_status",
        "session_message_sent",
        "session_message_deleted",
        "session_messages_delivered",
        "session_archived",
        "session_unarchived",
        "item_changed",
        "item_deleted",
        "prefix_changed",
        "settings_changed",
    ] {
        assert!(kinds.contains(&kind), "no {kind} notice in {kinds:?}");
    }
    for notice in &sent_live {
        let again = replayed
            .iter()
            .find(|r| r.event_id == notice.event_id)
            .unwrap_or_else(|| panic!("{} {} was not replayed", notice.kind, notice.event_id));
        assert_eq!(shape(again), shape(notice), "the replay of {}", notice.kind);
    }
}
