//! What agents' hooks report: the session's state, its turns and their
//! texts, sessions known only from their hooks, and the order of reports.

mod common;

use axum::http::StatusCode;
use common::{App, app, call, db};
use serde_json::{Value, json};

/// Posts a report of `event` with `seq`, and what `extra` adds to it.
async fn report(app: &App, session: &str, event: &str, seq: i64, extra: Value) -> StatusCode {
    let mut body = json!({"event": event, "seq": seq});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    call(
        app,
        "POST",
        &format!("/api/v1/sessions/claude-code/{session}/reports"),
        Some(body),
    )
    .await
    .0
}

/// The sessions every list shows.
async fn listed(app: &App) -> Vec<String> {
    let (_, listing) = call(app, "GET", "/api/v1/sessions", None).await;
    listing["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect()
}

fn state(app: &App, session: &str) -> Option<String> {
    db(app)
        .presence("claude-code", session)
        .unwrap()
        .map(|p| p.state)
}

#[tokio::test]
async fn a_session_known_only_from_its_hooks_is_hidden_until_it_reports_a_status() {
    let app = app();
    let origin = json!({"origin": {"repo": "acme/api", "folder": "/work/api"}});
    assert_eq!(
        report(&app, "s-1", "started", 1, origin).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(state(&app, "s-1").as_deref(), Some("idle"));
    assert!(listed(&app).await.is_empty());
    let (_, projects) = call(&app, "GET", "/api/v1/projects", None).await;
    assert!(!projects.to_string().contains("acme/api"), "{projects}");
    // its thread still opens, from a notification
    let (code, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(thread["session"]["folder"], "/work/api");

    let (code, _) = call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        Some(json!({"text": "Add rate limiting", "kind": "start"})),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(listed(&app).await, ["s-1"]);
}

#[tokio::test]
async fn hidden_sessions_go_after_seven_days_without_a_report_and_others_stay() {
    let app = app();
    report(&app, "hidden", "started", 1, json!({})).await;
    report(&app, "shown", "started", 1, json!({})).await;
    call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/shown/status",
        Some(json!({"text": "Working"})),
    )
    .await;
    let db = db(&app);
    let now = chrono::Utc::now();
    assert_eq!(
        db.delete_hidden_sessions(now - chrono::Duration::days(7))
            .unwrap(),
        0
    );
    assert_eq!(
        db.delete_hidden_sessions(now + chrono::Duration::seconds(1))
            .unwrap(),
        1
    );
    assert!(db.presence("claude-code", "hidden").unwrap().is_none());
    assert!(db.presence("claude-code", "shown").unwrap().is_some());
    assert_eq!(listed(&app).await, ["shown"]);
}

#[tokio::test]
async fn a_report_older_than_the_last_changes_nothing() {
    let app = app();
    report(
        &app,
        "s-1",
        "prompt",
        10,
        json!({"detail": {"prompt": "Go"}}),
    )
    .await;
    // late, and as old: both answered, neither taken
    for seq in [5, 10] {
        assert_eq!(
            report(&app, "s-1", "turn_ended", seq, json!({"end": "done"})).await,
            StatusCode::NO_CONTENT
        );
    }
    let presence = db(&app).presence("claude-code", "s-1").unwrap().unwrap();
    assert_eq!((presence.state.as_str(), presence.seq), ("working", 10));
    assert_eq!(
        db(&app).turns("claude-code", "s-1").unwrap()[0].ended_at,
        None
    );
}

#[tokio::test]
async fn a_turn_runs_from_the_prompt_to_the_end_with_its_texts() {
    let app = app();
    report(&app, "s-1", "started", 1, json!({})).await;
    report(
        &app,
        "s-1",
        "prompt",
        2,
        json!({"detail": {"prompt": "Add a health check"}}),
    )
    .await;
    report(&app, "s-1", "working", 3, json!({})).await;
    report(
        &app,
        "s-1",
        "needs_input",
        4,
        json!({"wait": "permission", "detail": {"tool": "Bash", "summary": "cargo test"}}),
    )
    .await;
    let waiting = db(&app).presence("claude-code", "s-1").unwrap().unwrap();
    assert_eq!(waiting.state, "needs_input");
    assert_eq!(waiting.wait_kind.as_deref(), Some("permission"));
    assert_eq!(waiting.wait_tool.as_deref(), Some("Bash"));
    assert_eq!(waiting.wait_summary.as_deref(), Some("cargo test"));
    assert_eq!(waiting.turn, 1);

    report(&app, "s-1", "working", 5, json!({})).await;
    let working = db(&app).presence("claude-code", "s-1").unwrap().unwrap();
    assert_eq!(
        (working.state.as_str(), working.wait_kind),
        ("working", None)
    );
    report(&app, "s-1", "working", 6, json!({})).await;
    // the same state keeps its start
    let still = db(&app).presence("claude-code", "s-1").unwrap().unwrap();
    assert_eq!(still.state_since, working.state_since);

    report(
        &app,
        "s-1",
        "turn_ended",
        7,
        json!({"end": "done", "detail": {"reply": "Added /health."}}),
    )
    .await;
    assert_eq!(state(&app, "s-1").as_deref(), Some("idle"));

    // a prompt while a turn is open ends it as interrupted
    report(
        &app,
        "s-1",
        "prompt",
        8,
        json!({"detail": {"prompt": "Now the docs"}}),
    )
    .await;
    report(
        &app,
        "s-1",
        "prompt",
        9,
        json!({"detail": {"prompt": "Stop, the README"}}),
    )
    .await;
    report(
        &app,
        "s-1",
        "ended",
        10,
        json!({"detail": {"reason": "prompt_input_exit"}}),
    )
    .await;

    let turns = db(&app).turns("claude-code", "s-1").unwrap();
    let summary: Vec<_> = turns
        .iter()
        .map(|t| {
            (
                t.turn,
                t.end_kind.as_deref(),
                t.prompt.as_deref(),
                t.reply.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                1,
                Some("done"),
                Some("Add a health check"),
                Some("Added /health.")
            ),
            (2, Some("interrupted"), Some("Now the docs"), None),
            (3, Some("ended"), Some("Stop, the README"), None),
        ]
    );
    let ended = db(&app).presence("claude-code", "s-1").unwrap().unwrap();
    assert_eq!(ended.state, "stopped");
    assert_eq!(ended.stop_reason.as_deref(), Some("ended"));
    assert_eq!(ended.turn, 3);
}

#[tokio::test]
async fn texts_are_left_out_when_the_person_turns_them_off() {
    let app = app();
    call(
        &app,
        "PATCH",
        "/api/v1/settings",
        Some(json!({"runtime": {"record_turns": false}})),
    )
    .await;
    report(
        &app,
        "s-1",
        "prompt",
        1,
        json!({"detail": {"prompt": "Secret plan"}}),
    )
    .await;
    report(
        &app,
        "s-1",
        "turn_ended",
        2,
        json!({"end": "done", "detail": {"reply": "Done"}}),
    )
    .await;
    let turns = db(&app).turns("claude-code", "s-1").unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(
        (turns[0].prompt.as_deref(), turns[0].reply.as_deref()),
        (None, None)
    );
    assert_eq!(turns[0].end_kind.as_deref(), Some("done"));
}

#[tokio::test]
async fn only_a_new_turn_counts_as_the_session_s_activity() {
    let app = app();
    call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        Some(json!({"text": "Working"})),
    )
    .await;
    call(
        &app,
        "PUT",
        "/api/v1/sessions/claude-code/s-1/archive",
        None,
    )
    .await;
    let last_at = || async {
        let (_, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
        (
            thread["session"]["last_at"].clone(),
            thread["session"]["archived_at"].clone(),
        )
    };
    let archived = last_at().await;
    assert!(!archived.1.is_null());
    report(
        &app,
        "s-1",
        "working",
        1,
        json!({"at": "2099-01-01T00:00:00Z"}),
    )
    .await;
    assert_eq!(last_at().await, archived);
    report(
        &app,
        "s-1",
        "prompt",
        2,
        json!({"at": "2099-01-01T00:00:00Z"}),
    )
    .await;
    let (last, archived_at) = last_at().await;
    assert_eq!(last, "2099-01-01T00:00:00.000000Z");
    assert!(archived_at.is_null());
}

#[tokio::test]
async fn a_report_that_is_not_one_is_refused() {
    let app = app();
    for body in [
        json!({"event": "dancing", "seq": 1}),
        json!({"event": "working"}),
        json!({"event": "working", "seq": 0}),
        json!({"event": "needs_input", "seq": 1}),
        json!({"event": "turn_ended", "seq": 1, "end": "maybe"}),
        json!({"event": "working", "seq": 1, "at": "yesterday"}),
    ] {
        let (code, _) = call(
            &app,
            "POST",
            "/api/v1/sessions/claude-code/s-1/reports",
            Some(body.clone()),
        )
        .await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }
    assert!(db(&app).presence("claude-code", "s-1").unwrap().is_none());
}

#[tokio::test]
async fn a_session_s_presence_and_turns_go_with_it() {
    let app = app();
    report(
        &app,
        "s-1",
        "prompt",
        1,
        json!({"detail": {"prompt": "Go"}}),
    )
    .await;
    report(
        &app,
        "s-1",
        "turn_ended",
        2,
        json!({"end": "done", "detail": {"reply": "Done"}}),
    )
    .await;
    let conn = rusqlite::Connection::open(app.state.config().db_path()).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    conn.execute("DELETE FROM sessions WHERE id = 's-1'", [])
        .unwrap();
    for table in ["session_presence", "session_turns", "session_turn_texts"] {
        let rows: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0, "{table}");
    }
}
