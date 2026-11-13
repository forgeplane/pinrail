//! What agents' hooks report: the session's state, its turns and their
//! texts, sessions known only from their hooks, and the order of reports.

mod common;

use axum::http::StatusCode;
use common::{App, app, call, db};
use serde_json::{Value, json};

/// When the hook fired for a report `s` seconds into a morning in the past,
/// so that the core never caps it.
fn at(s: i64) -> String {
    (chrono::DateTime::parse_from_rfc3339("2026-01-05T10:00:00Z").unwrap()
        + chrono::Duration::seconds(s))
    .to_utc()
    .to_rfc3339()
}

/// Posts a report of `event`, fired `s` seconds into the morning, and what
/// `extra` adds to it.
async fn report(app: &App, session: &str, event: &str, s: i64, extra: Value) -> StatusCode {
    let mut body = json!({"event": event, "at": at(s)});
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
async fn a_report_that_fired_before_the_last_still_opens_its_turn() {
    let app = app();
    // the prompt's hook ran late: the tool call after it arrived first
    report(&app, "s-1", "working", 11, json!({})).await;
    report(
        &app,
        "s-1",
        "prompt",
        10,
        json!({"detail": {"prompt": "Go"}}),
    )
    .await;
    let turns = db(&app).turns("claude-code", "s-1").unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].prompt.as_deref(), Some("Go"));
    assert_eq!(turns[0].started_at.to_rfc3339(), at(10));
    let presence = db(&app).presence("claude-code", "s-1").unwrap().unwrap();
    assert_eq!((presence.state.as_str(), presence.turn), ("working", 1));
    assert_eq!(presence.event_at.to_rfc3339(), at(11));
}

#[tokio::test]
async fn a_late_turn_end_closes_its_own_turn_with_its_reply() {
    let app = app();
    report(
        &app,
        "s-1",
        "prompt",
        10,
        json!({"detail": {"prompt": "First"}}),
    )
    .await;
    // the next prompt arrives before the end of the first turn
    report(
        &app,
        "s-1",
        "prompt",
        30,
        json!({"detail": {"prompt": "Second"}}),
    )
    .await;
    report(
        &app,
        "s-1",
        "turn_ended",
        20,
        json!({"end": "done", "detail": {"reply": "Did the first"}}),
    )
    .await;
    let turns = db(&app).turns("claude-code", "s-1").unwrap();
    let ends: Vec<_> = turns
        .iter()
        .map(|t| (t.turn, t.end_kind.as_deref(), t.reply.as_deref()))
        .collect();
    assert_eq!(
        ends,
        [(1, Some("done"), Some("Did the first")), (2, None, None)]
    );
    let presence = db(&app).presence("claude-code", "s-1").unwrap().unwrap();
    // the second turn still runs, and the first finished
    assert_eq!(presence.state, "working");
    assert!(presence.finished_at.is_some());
}

#[tokio::test]
async fn a_hook_s_clock_never_holds_a_session_back() {
    let app = app();
    let db = db(&app);
    let report = |event, at: &str| pinrail_core::db::Report {
        event,
        at: chrono::DateTime::parse_from_rfc3339(at).unwrap().to_utc(),
        process: None,
        origin: Default::default(),
        transcript_path: None,
        agent_turn: None,
        cut: false,
    };
    let now = chrono::Utc::now();
    // a time far ahead counts as now
    db.record_report(
        "claude-code",
        "s-1",
        &report(
            pinrail_core::db::ReportEvent::Working,
            "2099-01-01T00:00:00Z",
        ),
        true,
        now,
    )
    .unwrap();
    let presence = db.presence("claude-code", "s-1").unwrap().unwrap();
    assert!(presence.event_at <= chrono::Utc::now());
    // within 2 seconds, a report that fired earlier does not set the state
    let ended = || {
        report(
            pinrail_core::db::ReportEvent::TurnEnded {
                end: "done".into(),
                reply: None,
            },
            "2026-01-01T00:00:00Z",
        )
    };
    let taken = db
        .record_report(
            "claude-code",
            "s-1",
            &ended(),
            true,
            now + chrono::Duration::seconds(1),
        )
        .unwrap();
    assert!(!taken);
    assert_eq!(
        db.presence("claude-code", "s-1").unwrap().unwrap().state,
        "working"
    );
    // later than that, whatever its clock says, it does
    let taken = db
        .record_report(
            "claude-code",
            "s-1",
            &ended(),
            true,
            now + chrono::Duration::seconds(5),
        )
        .unwrap();
    assert!(taken);
    assert_eq!(
        db.presence("claude-code", "s-1").unwrap().unwrap().state,
        "idle"
    );
}

#[tokio::test]
async fn a_compaction_in_the_middle_of_a_turn_keeps_the_session_working() {
    let app = app();
    report(&app, "s-1", "started", 1, json!({"source": "startup"})).await;
    assert_eq!(state(&app, "s-1").as_deref(), Some("idle"));
    report(&app, "s-1", "prompt", 2, json!({})).await;
    report(&app, "s-1", "started", 3, json!({"source": "compact"})).await;
    assert_eq!(state(&app, "s-1").as_deref(), Some("working"));
    // a session that stopped and starts again is idle
    report(&app, "s-1", "ended", 4, json!({})).await;
    report(&app, "s-1", "started", 5, json!({"source": "resume"})).await;
    assert_eq!(state(&app, "s-1").as_deref(), Some("idle"));
}

#[tokio::test]
async fn finished_is_a_turn_that_ended_done_or_failed() {
    let app = app();
    let finished = |app: &App| {
        db(app)
            .presence("claude-code", "s-1")
            .unwrap()
            .unwrap()
            .finished_at
            .map(|t| t.to_rfc3339())
    };
    report(&app, "s-1", "prompt", 1, json!({})).await;
    // interrupted by the next prompt, and by the agent
    report(&app, "s-1", "prompt", 2, json!({})).await;
    report(&app, "s-1", "turn_ended", 3, json!({"end": "interrupted"})).await;
    assert_eq!(finished(&app), None);
    report(&app, "s-1", "prompt", 4, json!({})).await;
    report(
        &app,
        "s-1",
        "turn_ended",
        5,
        json!({"end": "failed", "detail": {"error": "rate_limit"}}),
    )
    .await;
    let failed = finished(&app);
    assert!(failed.is_some());
    let turns = db(&app).turns("claude-code", "s-1").unwrap();
    assert_eq!(turns[2].reply.as_deref(), Some("rate_limit"));
    // work that went on after the turn ended still finishes, and its
    // reply is the turn's
    report(
        &app,
        "s-1",
        "turn_ended",
        9,
        json!({"end": "done", "detail": {"reply": "Done after all"}}),
    )
    .await;
    assert!(finished(&app) > failed);
    let last = db(&app).turns("claude-code", "s-1").unwrap().pop().unwrap();
    assert_eq!(
        (last.end_kind.as_deref(), last.reply.as_deref()),
        (Some("done"), Some("Done after all"))
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
    // Claude Code's notification of the same wait carries no tool
    report(&app, "s-1", "needs_input", 4, json!({"wait": "permission"})).await;
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
    // a hook's clock is not the session's activity: the core's is
    report(
        &app,
        "s-1",
        "working",
        1,
        json!({"at": "2099-01-01T00:00:00Z"}),
    )
    .await;
    assert_eq!(last_at().await, archived);
    let before = chrono::Utc::now();
    report(
        &app,
        "s-1",
        "prompt",
        2,
        json!({"at": "2099-01-01T00:00:00Z"}),
    )
    .await;
    let (last, archived_at) = last_at().await;
    let last = chrono::DateTime::parse_from_rfc3339(last.as_str().unwrap()).unwrap();
    assert!(last >= before && last <= chrono::Utc::now(), "{last}");
    assert!(archived_at.is_null());
}

#[tokio::test]
async fn a_report_that_is_not_one_is_refused() {
    let app = app();
    for body in [
        json!({"event": "dancing"}),
        json!({}),
        json!({"event": "needs_input"}),
        json!({"event": "turn_ended", "end": "maybe"}),
        json!({"event": "started", "source": "nowhere"}),
        json!({"event": "working", "at": "yesterday"}),
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

#[tokio::test]
async fn an_interrupt_after_a_turn_finished_leaves_that_turn_as_it_was() {
    let app = app();
    report(&app, "s-1", "prompt", 1, json!({})).await;
    report(
        &app,
        "s-1",
        "turn_ended",
        2,
        json!({"end": "done", "detail": {"reply": "Done"}}),
    )
    .await;
    // Claude Code's idle prompt, a minute later
    report(&app, "s-1", "turn_ended", 62, json!({"end": "interrupted"})).await;
    let turn = db(&app).turns("claude-code", "s-1").unwrap().pop().unwrap();
    assert_eq!(
        (
            turn.end_kind.as_deref(),
            turn.ended_at.unwrap().to_rfc3339(),
            turn.reply.as_deref()
        ),
        (Some("done"), at(2), Some("Done"))
    );
}

#[tokio::test]
async fn reports_that_keep_arriving_out_of_order_never_freeze_the_state() {
    let app = app();
    let db = db(&app);
    let report = |event, at: String| pinrail_core::db::Report {
        event,
        at: chrono::DateTime::parse_from_rfc3339(&at).unwrap().to_utc(),
        process: None,
        origin: Default::default(),
        transcript_path: None,
        agent_turn: None,
        cut: false,
    };
    let now = chrono::Utc::now();
    // one report from a clock a minute ahead of the others
    db.record_report(
        "claude-code",
        "s-1",
        &report(pinrail_core::db::ReportEvent::Working, at(60)),
        true,
        now,
    )
    .unwrap();
    db.record_report(
        "claude-code",
        "s-1",
        &report(
            pinrail_core::db::ReportEvent::TurnEnded {
                end: "done".into(),
                reply: None,
            },
            at(1),
        ),
        true,
        now + chrono::Duration::seconds(3),
    )
    .unwrap();
    // reports a second apart that all fired before the one that set the
    // state: the window runs from that one, not from the last arrival, so
    // the third is taken
    for i in 0..3 {
        db.record_report(
            "claude-code",
            "s-1",
            &report(pinrail_core::db::ReportEvent::Working, at(0)),
            true,
            now + chrono::Duration::seconds(4 + i),
        )
        .unwrap();
    }
    assert_eq!(
        db.presence("claude-code", "s-1").unwrap().unwrap().state,
        "working"
    );
}

#[tokio::test]
async fn an_end_that_arrives_before_its_prompt_ends_its_own_turn() {
    let app = app();
    report(
        &app,
        "s-1",
        "prompt",
        1,
        json!({"detail": {"prompt": "First"}}),
    )
    .await;
    report(
        &app,
        "s-1",
        "turn_ended",
        2,
        json!({"end": "done", "detail": {"reply": "First done"}}),
    )
    .await;
    // the second turn's end arrives before its prompt
    report(
        &app,
        "s-1",
        "turn_ended",
        11,
        json!({"end": "failed", "detail": {"error": "rate_limit"}}),
    )
    .await;
    report(
        &app,
        "s-1",
        "prompt",
        10,
        json!({"detail": {"prompt": "Second"}}),
    )
    .await;
    let turns = db(&app).turns("claude-code", "s-1").unwrap();
    let second = &turns[1];
    assert_eq!(
        (
            second.turn,
            second.prompt.as_deref(),
            second.end_kind.as_deref(),
            second.reply.as_deref()
        ),
        (2, Some("Second"), Some("failed"), Some("rate_limit"))
    );
    // the first turn was cut short where the second began; its own end and
    // reply, which the early end replaced, are not known any more
    assert_eq!(turns[0].ended_at.unwrap().to_rfc3339(), at(10));
    assert_eq!(
        (turns[0].end_kind.as_deref(), turns[0].reply.as_deref()),
        (Some("interrupted"), None)
    );
}

#[tokio::test]
async fn a_late_permission_request_still_names_the_call_its_notification_did_not() {
    let app = app();
    report(&app, "s-1", "needs_input", 2, json!({"wait": "permission"})).await;
    report(
        &app,
        "s-1",
        "needs_input",
        1,
        json!({"wait": "permission", "detail": {"tool": "Bash", "summary": "npm test"}}),
    )
    .await;
    let waiting = db(&app).presence("claude-code", "s-1").unwrap().unwrap();
    assert_eq!(
        (
            waiting.wait_tool.as_deref(),
            waiting.wait_summary.as_deref()
        ),
        (Some("Bash"), Some("npm test"))
    );
    // a report that names a new call names all of it
    report(
        &app,
        "s-1",
        "needs_input",
        3,
        json!({"wait": "permission", "detail": {"tool": "Edit"}}),
    )
    .await;
    let waiting = db(&app).presence("claude-code", "s-1").unwrap().unwrap();
    assert_eq!(
        (waiting.wait_tool.as_deref(), waiting.wait_summary),
        (Some("Edit"), None)
    );
}

#[tokio::test]
async fn a_late_prompt_takes_its_place_among_the_turns() {
    let app = app();
    report(
        &app,
        "s-1",
        "prompt",
        1,
        json!({"detail": {"prompt": "First"}}),
    )
    .await;
    report(
        &app,
        "s-1",
        "prompt",
        20,
        json!({"detail": {"prompt": "Third"}}),
    )
    .await;
    report(
        &app,
        "s-1",
        "prompt",
        10,
        json!({"detail": {"prompt": "Second"}}),
    )
    .await;
    let turns: Vec<_> = db(&app)
        .turns("claude-code", "s-1")
        .unwrap()
        .into_iter()
        .map(|t| (t.turn, t.prompt.unwrap(), t.end_kind))
        .collect();
    assert_eq!(
        turns,
        [
            (1, "First".to_string(), Some("interrupted".to_string())),
            (2, "Second".to_string(), Some("interrupted".to_string())),
            (3, "Third".to_string(), None),
        ]
    );
    let presence = db(&app).presence("claude-code", "s-1").unwrap().unwrap();
    assert_eq!(presence.turn, 3);
    // an end at the very moment a turn starts is not that turn's
    report(&app, "s-1", "turn_ended", 20, json!({"end": "done"})).await;
    let last = db(&app).turns("claude-code", "s-1").unwrap().pop().unwrap();
    assert_eq!(last.end_kind, None);
}

#[tokio::test]
async fn a_prompt_whose_clock_stepped_back_moves_no_turn_s_end() {
    let app = app();
    let db = db(&app);
    let report = |event, at: String| pinrail_core::db::Report {
        event,
        at: chrono::DateTime::parse_from_rfc3339(&at).unwrap().to_utc(),
        process: None,
        origin: Default::default(),
        transcript_path: None,
        agent_turn: None,
        cut: false,
    };
    let now = chrono::Utc::now();
    let prompt = || pinrail_core::db::ReportEvent::Prompt { prompt: None };
    let done = pinrail_core::db::ReportEvent::TurnEnded {
        end: "done".into(),
        reply: Some("Done".into()),
    };
    let late = now + chrono::Duration::seconds(21);
    db.record_report("claude-code", "s-1", &report(prompt(), at(0)), true, now)
        .unwrap();
    db.record_report("claude-code", "s-1", &report(done, at(50)), true, now)
        .unwrap();
    // 20 seconds later, from a clock that stepped back 30 seconds
    db.record_report("claude-code", "s-1", &report(prompt(), at(40)), true, late)
        .unwrap();
    let turns = db.turns("claude-code", "s-1").unwrap();
    assert_eq!(turns[0].reply.as_deref(), Some("Done"));
    assert_eq!(turns[1].end_kind, None);
    assert_eq!(
        db.presence("claude-code", "s-1").unwrap().unwrap().state,
        "working"
    );
}

#[tokio::test]
async fn an_early_end_without_a_reply_takes_no_other_turn_s_reply() {
    let app = app();
    report(&app, "s-1", "prompt", 1, json!({})).await;
    report(
        &app,
        "s-1",
        "turn_ended",
        2,
        json!({"end": "done", "detail": {"reply": "Added /health"}}),
    )
    .await;
    report(&app, "s-1", "turn_ended", 11, json!({"end": "failed"})).await;
    report(&app, "s-1", "prompt", 10, json!({})).await;
    let turns = db(&app).turns("claude-code", "s-1").unwrap();
    assert_eq!(
        (turns[1].end_kind.as_deref(), turns[1].reply.as_deref()),
        (Some("failed"), None)
    );
}

#[tokio::test]
async fn turns_do_not_overlap_after_a_late_prompt() {
    let app = app();
    report(&app, "s-1", "prompt", 10, json!({})).await;
    report(&app, "s-1", "prompt", 30, json!({})).await;
    report(&app, "s-1", "prompt", 20, json!({})).await;
    let spans: Vec<_> = db(&app)
        .turns("claude-code", "s-1")
        .unwrap()
        .iter()
        .map(|t| {
            (
                t.started_at.to_rfc3339(),
                t.ended_at.map(|e| e.to_rfc3339()),
            )
        })
        .collect();
    assert_eq!(
        spans,
        [
            (at(10), Some(at(20))),
            (at(20), Some(at(30))),
            (at(30), None)
        ]
    );
}

#[tokio::test]
async fn a_turn_keeps_the_agent_s_id_its_transcript_and_whether_its_texts_were_cut() {
    let app = app();
    let path = "/Users/you/.claude/projects/app/s-1.jsonl";
    report(
        &app,
        "s-1",
        "prompt",
        1,
        json!({"transcript_path": path, "detail": {"prompt": "Go", "turn_id": "p-1", "prompt_cut": true}}),
    )
    .await;
    report(
        &app,
        "s-1",
        "prompt",
        2,
        json!({"detail": {"prompt": "Next", "turn_id": "p-2"}}),
    )
    .await;
    // the end of the first turn arrives late, and names its turn
    let long = "y".repeat(40_000);
    report(
        &app,
        "s-1",
        "turn_ended",
        3,
        json!({"end": "done", "detail": {"reply": long, "turn_id": "p-1"}}),
    )
    .await;
    let turns = db(&app).turns("claude-code", "s-1").unwrap();
    let first = &turns[0];
    assert_eq!(first.agent_turn.as_deref(), Some("p-1"));
    assert_eq!(
        (first.end_kind.as_deref(), first.prompt_cut),
        (Some("done"), true)
    );
    assert_eq!(first.reply.as_deref().map(str::len), Some(32_000));
    assert!(first.reply_cut);
    assert_eq!(
        (turns[1].end_kind.as_deref(), turns[1].prompt_cut),
        (None, false)
    );
    let stored: Option<String> = rusqlite::Connection::open(app.state.config().db_path())
        .unwrap()
        .query_row(
            "SELECT transcript_path FROM sessions WHERE id = 's-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored.as_deref(), Some(path));
}
