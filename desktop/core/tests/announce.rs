//! Which notices raise a system notification: the core decides, from the
//! notification an event changed and the person's settings.

use serde_json::{Value, json};

mod common;
use common::{App, app, call};

const REPO: &str = "forgeplane/pinrail";

async fn agent(app: &App, method: &str, path: &str, body: Value) -> Value {
    let mut body = body;
    body["by"] = json!("claude-code");
    body["session"] = json!("s-1");
    let (code, answer) = common::call_as(app, method, path, Some(body), &[]).await;
    assert!(code.is_success(), "{method} {path}: {code} {answer}");
    answer
}

async fn settings(app: &App, change: Value) {
    let (code, answer) = call(app, "PATCH", "/api/v1/settings", Some(change)).await;
    assert!(code.is_success(), "{answer}");
}

/// The titles of what `act` announced.
async fn announced<F: std::future::Future<Output = ()>>(app: &App, act: F) -> Vec<String> {
    let mut live = app.state.events().subscribe();
    act.await;
    std::iter::from_fn(|| live.try_recv().ok())
        .filter_map(|n| n.announce)
        .map(|a| a["title"].as_str().unwrap_or_default().to_string())
        .collect()
}

async fn question(app: &App, title: &str) {
    agent(
        app,
        "POST",
        "/api/v1/reviews",
        json!({"kind": "question", "title": title, "origin": {"repo": REPO},
               "requested_by": "claude-code", "session": "s-1",
               "payload": {"questions": [{"id": "q1", "question": title, "answer": "yes-no"}]}}),
    )
    .await;
}

async fn review(app: &App, title: &str) {
    agent(
        app,
        "POST",
        "/api/v1/reviews",
        json!({"plugin": "list", "title": title, "origin": {"repo": REPO},
               "requested_by": "claude-code",
               "payload": {"groups": [{"title": "a", "items": [{"id": 1, "title": "one"}]}]}}),
    )
    .await;
}

async fn comment(app: &App) {
    agent(
        app,
        "POST",
        "/api/v1/items",
        json!({"repo": REPO, "title": "Themes"}),
    )
    .await;
    agent(
        app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        json!({"body": "The loader is ready."}),
    )
    .await;
}

async fn finish(app: &App) {
    let at = |ms: i64| (chrono::Utc::now() + chrono::Duration::milliseconds(ms)).to_rfc3339();
    for body in [
        json!({"event": "prompt", "at": at(0)}),
        json!({"event": "turn_ended", "end": "done", "at": at(10), "detail": {"reply": "Done."}}),
    ] {
        call(
            app,
            "POST",
            "/api/v1/sessions/claude-code/s-1/reports",
            Some(body),
        )
        .await;
    }
}

#[tokio::test]
async fn by_default_a_review_and_a_question_are_announced_and_nothing_else() {
    let app = app();
    assert_eq!(
        announced(&app, review(&app, "Rate limiter")).await,
        ["Rate limiter"]
    );
    assert_eq!(
        announced(&app, question(&app, "Deploy?")).await,
        ["Deploy?"]
    );
    assert!(announced(&app, comment(&app)).await.is_empty());
    assert!(announced(&app, finish(&app)).await.is_empty());
}

#[tokio::test]
async fn nothing_is_announced_while_notifications_are_off_paused_or_quiet() {
    let app = app();
    settings(&app, json!({"notifications": {"enabled": false}})).await;
    assert!(announced(&app, review(&app, "Off")).await.is_empty());

    let later = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    settings(
        &app,
        json!({"notifications": {"enabled": true, "paused_until": later}}),
    )
    .await;
    assert!(announced(&app, review(&app, "Paused")).await.is_empty());

    // quiet from an hour ago to an hour from now
    let now = chrono::Local::now();
    let clock = |h: i64| {
        (now + chrono::Duration::hours(h))
            .format("%H:%M")
            .to_string()
    };
    settings(
        &app,
        json!({"notifications": {"paused_until": null, "quiet_hours": {"from": clock(-1), "to": clock(1)}}}),
    )
    .await;
    assert!(announced(&app, review(&app, "Quiet")).await.is_empty());

    settings(&app, json!({"notifications": {"quiet_hours": null}})).await;
    assert_eq!(announced(&app, review(&app, "On")).await, ["On"]);
}

#[tokio::test]
async fn a_muted_plugin_and_a_kind_off_in_the_inbox_are_not_announced() {
    let app = app();
    settings(&app, json!({"notifications": {"muted_plugins": ["list"]}})).await;
    assert!(
        announced(&app, review(&app, "Muted plugin"))
            .await
            .is_empty()
    );
    // a question has no plugin to mute
    assert_eq!(
        announced(&app, question(&app, "Deploy?")).await,
        ["Deploy?"]
    );

    // announced, but off in the inbox: never announced
    settings(
        &app,
        json!({"notifications": {"announce": {"item_commented": true}},
               "inbox": {"show": {"item_commented": false}}}),
    )
    .await;
    assert!(announced(&app, comment(&app)).await.is_empty());
}

#[tokio::test]
async fn each_announce_setting_turns_its_kind_on() {
    let app = app();
    settings(
        &app,
        json!({"notifications": {"announce": {
            "review_requested": false, "agent_finished": true, "item_commented": true, "agent_waits": true
        }}}),
    )
    .await;
    assert!(announced(&app, review(&app, "Off")).await.is_empty());
    assert_eq!(announced(&app, comment(&app)).await, ["PIN-1 Themes"]);
    let finished = announced(&app, finish(&app)).await;
    assert_eq!(finished.len(), 1, "{finished:?}");
    let waits = announced(&app, async {
        let at = (chrono::Utc::now() + chrono::Duration::seconds(1)).to_rfc3339();
        call(
            &app,
            "POST",
            "/api/v1/sessions/claude-code/s-1/reports",
            Some(json!({"event": "needs_input", "at": at, "wait": "permission"})),
        )
        .await;
    })
    .await;
    assert_eq!(waits.len(), 1, "{waits:?}");
}
