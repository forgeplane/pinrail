//! The usage data the core records where each event happens: decisions,
//! answers, discards, sessions, work items, plugins, the features the core
//! sees and the window reports, and the CLI's commands.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

mod common;
use common::{App, app, call};

/// An app whose person has seen the telemetry notice.
async fn counting() -> App {
    let app = app();
    let (status, _) = call(
        &app,
        "PATCH",
        "/api/v1/settings",
        Some(json!({"telemetry": {"notice_shown": "2026-11-10T09:00:00Z"}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    app
}

/// The events that wait to be sent, as `name` and `fields`, without the
/// time.
async fn waiting(app: &App) -> Vec<Value> {
    let (_, status) = call(app, "GET", "/api/v1/telemetry", None).await;
    status["waiting"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| json!({"name": e["name"], "fields": e["fields"]}))
        .collect()
}

/// The waiting events called `name`, by their fields.
async fn fields_of(app: &App, name: &str) -> Vec<Value> {
    waiting(app)
        .await
        .into_iter()
        .filter(|e| e["name"] == name)
        .map(|e| e["fields"].clone())
        .collect()
}

/// A request as the `pinrail` CLI makes it, running `command` under
/// `agent`.
async fn cli(
    app: &App,
    method: &str,
    path: &str,
    body: Option<Value>,
    command: &str,
    agent: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:4747")
        .header("content-type", "application/json")
        .header("x-pinrail-client", "cli")
        .header("x-pinrail-command", command)
        .header("x-pinrail-agent", agent)
        .body(
            body.map(|b| Body::from(b.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn submit(app: &App, body: Value) -> String {
    let (status, review) = call(app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    review["id"].as_str().unwrap().to_string()
}

fn list_review(title: &str, revises: Option<&str>) -> Value {
    json!({
        "plugin": "list",
        "title": title,
        "revises": revises,
        "payload": {"groups": [{"title": "g", "items": [{"id": 1, "title": "one"}, {"id": 2, "title": "two"}]}]}
    })
}

#[tokio::test]
async fn nothing_is_counted_before_the_notice_was_shown() {
    let app = app();
    let id = submit(&app, list_review("Before the notice", None)).await;
    call(
        &app,
        "POST",
        &format!("/api/v1/reviews/{id}/discard"),
        Some(json!({})),
    )
    .await;
    assert_eq!(waiting(&app).await, Vec::<Value>::new());
}

#[tokio::test]
async fn showing_the_notice_counts_the_first_launch() {
    let app = counting().await;
    assert_eq!(
        fields_of(&app, "app_started").await,
        [json!({"first_run": true})]
    );
}

#[tokio::test]
async fn a_decision_counts_its_outcome_its_rounds_and_its_wait() {
    let app = counting().await;
    let first = submit(&app, list_review("Rename the tables", None)).await;
    let second = submit(&app, list_review("Rename the tables", Some(&first))).await;
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/reviews/{second}/decision"),
        Some(json!({"data": {
            "decisions": [{"id": 1, "action": "accept"}, {"id": 2, "action": "reject"}],
            "undecided": []
        }})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let decided = fields_of(&app, "review_decided").await;
    assert_eq!(decided.len(), 1);
    assert_eq!(decided[0]["plugin"], "list");
    assert_eq!(decided[0]["outcome"], "changes");
    assert_eq!(decided[0]["rounds"], 2);
    assert!(decided[0]["seconds_waiting"].as_u64().unwrap() < 60);

    let third = submit(&app, list_review("Accept them all", None)).await;
    call(
        &app,
        "POST",
        &format!("/api/v1/reviews/{third}/decision"),
        Some(json!({"data": {
            "decisions": [{"id": 1, "action": "accept"}, {"id": 2, "action": "accept"}],
            "undecided": []
        }})),
    )
    .await;
    let decided = fields_of(&app, "review_decided").await;
    assert_eq!(decided[1]["outcome"], "approved");
    assert_eq!(decided[1]["rounds"], 1);
}

#[tokio::test]
async fn answers_count_the_questions_left_and_the_suggested_reply() {
    let app = counting().await;
    let ask = |suggested: Option<&str>| {
        json!({
            "kind": "question",
            "title": "Two questions",
            "requested_by": "claude-code",
            "session": "s-1",
            "payload": {
                "questions": [
                    {"id": "q1", "question": "Commit it?", "answer": "yes-no"},
                    {"id": "q2", "question": "What next?", "answer": "text"}
                ],
                "suggested_reply": suggested
            }
        })
    };
    let answers = json!({"answers": [{"id": "q1", "yes": true}, {"id": "q2", "unanswered": true}]});
    for (suggested, note) in [
        (Some("yes, commit"), "yes, commit"),
        (Some("yes, commit"), "not yet"),
        (None, "fine"),
    ] {
        let mut body = ask(suggested);
        if suggested.is_none() {
            body["payload"]
                .as_object_mut()
                .unwrap()
                .remove("suggested_reply");
        }
        let id = submit(&app, body).await;
        let (status, response) = call(
            &app,
            "POST",
            &format!("/api/v1/reviews/{id}/decision"),
            Some(json!({"data": answers, "agent_note": note})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{response}");
    }
    let used = |used: bool| json!({"questions": 2, "unanswered": 1, "suggested_reply_used": used});
    assert_eq!(
        fields_of(&app, "questions_answered").await,
        [used(true), used(false), used(false)]
    );
    assert_eq!(fields_of(&app, "review_decided").await, Vec::<Value>::new());
}

#[tokio::test]
async fn a_discard_counts_its_plugin() {
    let app = counting().await;
    let id = submit(&app, list_review("Not this", None)).await;
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/reviews/{id}/discard"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        fields_of(&app, "review_discarded").await,
        [json!({"plugin": "list"})]
    );
}

#[tokio::test]
async fn a_session_counts_once_under_its_agent() {
    let app = counting().await;
    for (agent, session, text) in [
        ("claude-code", "s-1", "Start"),
        ("claude-code", "s-1", "Go on"),
        ("my own agent", "s-2", "Start"),
    ] {
        let (status, body) = call(
            &app,
            "POST",
            &format!(
                "/api/v1/sessions/{}/{session}/status",
                agent.replace(' ', "%20")
            ),
            Some(json!({"text": text})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    assert_eq!(
        fields_of(&app, "session_started").await,
        [json!({"agent": "claude-code"}), json!({"agent": "other"})]
    );
}

#[tokio::test]
async fn work_item_changes_count_who_made_them() {
    let app = counting().await;
    let as_agent = json!({"by": "claude-code", "session": "s-1"});
    let (status, item) = cli(
        &app,
        "POST",
        "/api/v1/items",
        Some(json!({"repo": "acme/api", "title": "Rename the tables", "by": "claude-code", "session": "s-1"})),
        "items",
        "claude-code",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{item}");
    let key = item["key"].as_str().unwrap().to_string();
    let mut claim = as_agent.clone();
    claim["claim"] = json!("claim");
    let (status, body) = cli(
        &app,
        "PATCH",
        &format!("/api/v1/items/{key}"),
        Some(claim),
        "items",
        "claude-code",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = cli(
        &app,
        "POST",
        &format!("/api/v1/items/{key}/done"),
        Some(as_agent),
        "items",
        "claude-code",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // the person, in the app
    let (_, other) = call(
        &app,
        "POST",
        "/api/v1/items",
        Some(json!({"repo": "acme/api", "title": "Hold this"})),
    )
    .await;
    let other = other["key"].as_str().unwrap().to_string();
    let (status, body) = call(
        &app,
        "PUT",
        &format!("/api/v1/items/{other}/queue"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = call(
        &app,
        "PATCH",
        &format!("/api/v1/items/{other}"),
        Some(json!({"status": "on_hold"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let change = |change: &str, by_agent: bool| json!({"change": change, "by_agent": by_agent});
    assert_eq!(
        fields_of(&app, "item_changed").await,
        [
            change("added", true),
            change("claimed", true),
            change("done", true),
            change("added", false),
            change("queued", false),
            change("held", false),
        ]
    );
    // the CLI's command counts once a day
    assert_eq!(
        fields_of(&app, "cli_used").await,
        [json!({"command": "items", "agent": "claude-code"})]
    );
}

#[tokio::test]
async fn a_plugin_counts_when_it_is_installed_and_removed() {
    let app = counting().await;
    let (status, body) = call(&app, "DELETE", "/api/v1/plugins/feedback", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    common::install(&app, &["forgeplane/feedback"]);
    let plugin =
        |removed: bool| json!({"plugin": "feedback", "official": true, "removed": removed});
    assert_eq!(
        fields_of(&app, "plugin_installed").await,
        [plugin(true), plugin(false)]
    );
}

#[tokio::test]
async fn the_core_counts_the_features_it_sees() {
    let app = counting().await;
    let id = submit(&app, list_review("Star me", None)).await;
    call(&app, "PUT", &format!("/api/v1/reviews/{id}/star"), None).await;
    call(&app, "DELETE", &format!("/api/v1/reviews/{id}/star"), None).await;
    call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        Some(json!({"text": "Start"})),
    )
    .await;
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/messages",
        Some(json!({"text": "Use the v2 endpoint"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    for change in [
        json!({"appearance": {"dark_theme": "dark"}}),
        json!({"appearance": {"dark_theme": "my-theme"}}),
        json!({"notifications": {"paused_until": "2026-11-10T12:00:00Z"}}),
        json!({"notifications": {"paused_until": null}}),
    ] {
        let (status, body) = call(&app, "PATCH", "/api/v1/settings", Some(change)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let features: Vec<Value> = fields_of(&app, "feature_used")
        .await
        .into_iter()
        .map(|f| f["feature"].clone())
        .collect();
    assert_eq!(
        features,
        [
            "round_starred",
            "session_message",
            "custom_theme",
            "notifications_paused"
        ]
    );
}

#[tokio::test]
async fn the_window_reports_only_the_features_the_core_lists() {
    let app = counting().await;
    for (feature, expected) in [
        ("command_palette", StatusCode::NO_CONTENT),
        ("page_items", StatusCode::NO_CONTENT),
        ("command_palette", StatusCode::NO_CONTENT),
        (
            "the review titled Acme payroll",
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        let (status, body) = call(
            &app,
            "POST",
            "/api/v1/telemetry/features",
            Some(json!({"feature": feature})),
        )
        .await;
        assert_eq!(status, expected, "{feature}: {body}");
    }
    assert_eq!(
        fields_of(&app, "feature_used").await,
        [
            json!({"feature": "command_palette"}),
            json!({"feature": "page_items"})
        ]
    );
}

#[tokio::test]
async fn an_unknown_command_or_an_odd_agent_is_counted_as_other() {
    let app = counting().await;
    cli(
        &app,
        "GET",
        "/api/v1/info",
        None,
        "frobnicate",
        "claude-code",
    )
    .await;
    cli(
        &app,
        "GET",
        "/api/v1/info",
        None,
        "list",
        "my agent/with a path",
    )
    .await;
    assert_eq!(
        fields_of(&app, "cli_used").await,
        [
            json!({"command": "other", "agent": "claude-code"}),
            json!({"command": "list", "agent": "other"})
        ]
    );
}

#[tokio::test]
async fn an_uncaught_error_of_the_window_is_kept_scrubbed_while_crash_reports_are_on() {
    let app = counting().await;
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/telemetry/crashes",
        Some(json!({
            "message": "Cannot read properties of undefined (reading 'title')",
            "stack": "TypeError: x\n    at render (http://127.0.0.1:5173/src/screens/Inbox.tsx:12:5)"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, telemetry) = call(&app, "GET", "/api/v1/telemetry", None).await;
    let crash = &telemetry["crash"];
    assert_eq!(crash["kind"], "ui");
    assert_eq!(
        crash["message"],
        "Cannot read properties of undefined (reading 'title')"
    );
    assert_eq!(crash["stack"], "TypeError: x\n    at render (…)");

    // off: the report goes, and no other is kept
    call(
        &app,
        "PATCH",
        "/api/v1/settings",
        Some(json!({"telemetry": {"crashes": false}})),
    )
    .await;
    call(
        &app,
        "POST",
        "/api/v1/telemetry/crashes",
        Some(json!({"message": "again", "stack": ""})),
    )
    .await;
    let (_, telemetry) = call(&app, "GET", "/api/v1/telemetry", None).await;
    assert_eq!(telemetry["crash"], Value::Null);
}
