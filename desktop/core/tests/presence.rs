//! What agents' hooks report: the session's state, its turns and their
//! texts, sessions known only from their hooks, and the order of reports.

mod common;

use axum::http::StatusCode;
use common::{App, app, call, db};
use serde_json::{Value, json};

/// A report recorded as the service records it, with the presence
/// service's transition; answers whether it set the state.
trait Record {
    fn record(
        &self,
        agent: &str,
        id: &str,
        report: &pinrail_core::db::Report,
        keep_texts: bool,
        now: chrono::DateTime<chrono::Utc>,
    ) -> rusqlite::Result<bool>;
}

impl Record for pinrail_core::db::Db {
    fn record(
        &self,
        agent: &str,
        id: &str,
        report: &pinrail_core::db::Report,
        keep_texts: bool,
        now: chrono::DateTime<chrono::Utc>,
    ) -> rusqlite::Result<bool> {
        let transition = |event: &pinrail_core::db::ReportEvent,
                          stored: Option<&pinrail_core::db::StoredState>,
                          process: Option<&(i64, String)>| {
            pinrail_core::presence::next(event, stored, process, &pinrail_core::presence::alive)
        };
        self.record_report(agent, id, report, keep_texts, now, &transition)
            .map(|r| r.taken)
    }
}

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
        .map(|s| s["agent_session"].as_str().unwrap().to_string())
        .collect()
}

fn state(app: &App, session: &str) -> Option<String> {
    db(app)
        .presence("claude-code", &common::sid(app, "claude-code", session))
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
    let (code, thread) = call(
        &app,
        "GET",
        &format!(
            "/api/v1/sessions/claude-code/{}",
            common::sid(&app, "claude-code", "s-1")
        ),
        None,
    )
    .await;
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
async fn a_hook_report_fills_the_repository_a_session_was_created_without() {
    let app = app();
    // created with only a folder, as a session Pinrail starts is
    let (code, _) = call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        Some(json!({"text": "Starting", "origin": {"folder": "/work/api"}})),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    let repo = |thread: &Value| thread["session"]["repo"].clone();
    let (_, thread) = call(
        &app,
        "GET",
        &format!(
            "/api/v1/sessions/claude-code/{}",
            common::sid(&app, "claude-code", "s-1")
        ),
        None,
    )
    .await;
    assert_eq!(repo(&thread), Value::Null);

    let origin = json!({"origin": {"repo": "acme/api", "ref": "main", "folder": "/work/api"}});
    assert_eq!(
        report(&app, "s-1", "prompt", 1, origin).await,
        StatusCode::NO_CONTENT
    );
    let (_, thread) = call(
        &app,
        "GET",
        &format!(
            "/api/v1/sessions/claude-code/{}",
            common::sid(&app, "claude-code", "s-1")
        ),
        None,
    )
    .await;
    assert_eq!(repo(&thread), "acme/api");
    assert_eq!(thread["session"]["ref"], "main");

    // what the agent reported itself stays
    let other = json!({"origin": {"repo": "acme/web", "ref": "dev"}});
    report(&app, "s-1", "prompt", 2, other).await;
    let (_, thread) = call(
        &app,
        "GET",
        &format!(
            "/api/v1/sessions/claude-code/{}",
            common::sid(&app, "claude-code", "s-1")
        ),
        None,
    )
    .await;
    assert_eq!(repo(&thread), "acme/api");
    assert_eq!(thread["session"]["ref"], "main");
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
    assert!(
        db.presence("claude-code", &common::sid(&app, "claude-code", "hidden"))
            .unwrap()
            .is_none()
    );
    assert!(
        db.presence("claude-code", &common::sid(&app, "claude-code", "shown"))
            .unwrap()
            .is_some()
    );
    assert_eq!(listed(&app).await, ["shown"]);
}

#[tokio::test]
async fn a_prompt_is_kept_without_the_pinrail_note_before_it() {
    let app = app();
    report(
        &app,
        "s-1",
        "prompt",
        10,
        json!({"detail": {"prompt": "<pinrail>\nThis session was started from Pinrail.\n</pinrail>\n\nFix the login"}}),
    )
    .await;
    let turns = db(&app)
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap();
    assert_eq!(turns[0].prompt.as_deref(), Some("Fix the login"));
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
    let turns = db(&app)
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].prompt.as_deref(), Some("Go"));
    assert_eq!(turns[0].started_at.to_rfc3339(), at(10));
    let presence = db(&app)
        .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .unwrap();
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
    let turns = db(&app)
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap();
    let ends: Vec<_> = turns
        .iter()
        .map(|t| (t.turn, t.end_kind.as_deref(), t.reply()))
        .collect();
    assert_eq!(
        ends,
        [(1, Some("done"), Some("Did the first")), (2, None, None)]
    );
    let presence = db(&app)
        .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .unwrap();
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
        source: Default::default(),
    };
    let now = chrono::Utc::now();
    // a time far ahead counts as now
    db.record(
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
    let presence = db
        .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .unwrap();
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
        .record(
            "claude-code",
            "s-1",
            &ended(),
            true,
            now + chrono::Duration::seconds(1),
        )
        .unwrap();
    assert!(!taken);
    assert_eq!(
        db.presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
            .unwrap()
            .unwrap()
            .state,
        "working"
    );
    // later than that, whatever its clock says, it does
    let taken = db
        .record(
            "claude-code",
            "s-1",
            &ended(),
            true,
            now + chrono::Duration::seconds(5),
        )
        .unwrap();
    assert!(taken);
    assert_eq!(
        db.presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
            .unwrap()
            .unwrap()
            .state,
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
            .presence("claude-code", &common::sid(app, "claude-code", "s-1"))
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
    let turns = db(&app)
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap();
    assert_eq!(turns[2].reply(), Some("rate_limit"));
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
    let last = db(&app)
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        (last.end_kind.as_deref(), last.reply()),
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
    let waiting = db(&app)
        .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .unwrap();
    assert_eq!(waiting.state, "needs_input");
    assert_eq!(waiting.wait_kind.as_deref(), Some("permission"));
    assert_eq!(waiting.wait_tool.as_deref(), Some("Bash"));
    assert_eq!(waiting.wait_summary.as_deref(), Some("cargo test"));
    assert_eq!(waiting.turn, 1);

    report(&app, "s-1", "working", 5, json!({})).await;
    let working = db(&app)
        .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .unwrap();
    assert_eq!(
        (working.state.as_str(), working.wait_kind),
        ("working", None)
    );
    report(&app, "s-1", "working", 6, json!({})).await;
    // the same state keeps its start
    let still = db(&app)
        .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .unwrap();
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

    let turns = db(&app)
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap();
    let summary: Vec<_> = turns
        .iter()
        .map(|t| {
            (
                t.turn,
                t.end_kind.as_deref(),
                t.prompt.as_deref(),
                t.reply(),
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
    let ended = db(&app)
        .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .unwrap();
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
    let turns = db(&app)
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!((turns[0].prompt.as_deref(), turns[0].reply()), (None, None));
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
        &format!(
            "/api/v1/sessions/claude-code/{}/archive",
            common::sid(&app, "claude-code", "s-1")
        ),
        None,
    )
    .await;
    let last_at = || async {
        let (_, thread) = call(
            &app,
            "GET",
            &format!(
                "/api/v1/sessions/claude-code/{}",
                common::sid(&app, "claude-code", "s-1")
            ),
            None,
        )
        .await;
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
    assert!(
        db(&app)
            .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
            .unwrap()
            .is_none()
    );
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
    conn.execute(
        "DELETE FROM sessions WHERE id = ?1",
        [common::sid(&app, "claude-code", "s-1")],
    )
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
    let turn = db(&app)
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        (
            turn.end_kind.as_deref(),
            turn.ended_at.unwrap().to_rfc3339(),
            turn.reply()
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
        source: Default::default(),
    };
    let now = chrono::Utc::now();
    // one report from a clock a minute ahead of the others
    db.record(
        "claude-code",
        "s-1",
        &report(pinrail_core::db::ReportEvent::Working, at(60)),
        true,
        now,
    )
    .unwrap();
    db.record(
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
        db.record(
            "claude-code",
            "s-1",
            &report(pinrail_core::db::ReportEvent::Working, at(0)),
            true,
            now + chrono::Duration::seconds(4 + i),
        )
        .unwrap();
    }
    assert_eq!(
        db.presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
            .unwrap()
            .unwrap()
            .state,
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
    let turns = db(&app)
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap();
    let second = &turns[1];
    assert_eq!(
        (
            second.turn,
            second.prompt.as_deref(),
            second.end_kind.as_deref(),
            second.reply()
        ),
        (2, Some("Second"), Some("failed"), Some("rate_limit"))
    );
    // the first turn was cut short where the second began; its own end and
    // reply, which the early end replaced, are not known any more
    assert_eq!(turns[0].ended_at.unwrap().to_rfc3339(), at(10));
    assert_eq!(
        (turns[0].end_kind.as_deref(), turns[0].reply()),
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
    let waiting = db(&app)
        .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .unwrap();
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
    let waiting = db(&app)
        .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .unwrap();
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
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
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
    let presence = db(&app)
        .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .unwrap();
    assert_eq!(presence.turn, 3);
    // an end at the very moment a turn starts is not that turn's
    report(&app, "s-1", "turn_ended", 20, json!({"end": "done"})).await;
    let last = db(&app)
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .pop()
        .unwrap();
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
        source: Default::default(),
    };
    let now = chrono::Utc::now();
    let prompt = || pinrail_core::db::ReportEvent::Prompt { prompt: None };
    let done = pinrail_core::db::ReportEvent::TurnEnded {
        end: "done".into(),
        reply: Some("Done".into()),
    };
    let late = now + chrono::Duration::seconds(21);
    db.record("claude-code", "s-1", &report(prompt(), at(0)), true, now)
        .unwrap();
    db.record("claude-code", "s-1", &report(done, at(50)), true, now)
        .unwrap();
    // 20 seconds later, from a clock that stepped back 30 seconds
    db.record("claude-code", "s-1", &report(prompt(), at(40)), true, late)
        .unwrap();
    let turns = db
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap();
    assert_eq!(turns[0].reply(), Some("Done"));
    assert_eq!(turns[1].end_kind, None);
    assert_eq!(
        db.presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
            .unwrap()
            .unwrap()
            .state,
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
    let turns = db(&app)
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap();
    assert_eq!(
        (turns[1].end_kind.as_deref(), turns[1].reply()),
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
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
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
    let turns = db(&app)
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap();
    let first = &turns[0];
    assert_eq!(first.agent_turn.as_deref(), Some("p-1"));
    assert_eq!(
        (first.end_kind.as_deref(), first.prompt_cut),
        (Some("done"), true)
    );
    assert_eq!(first.reply().map(str::len), Some(32_000));
    assert!(first.replies[0].cut);
    assert_eq!(
        (turns[1].end_kind.as_deref(), turns[1].prompt_cut),
        (None, false)
    );
    let stored: Option<String> = rusqlite::Connection::open(app.state.config().db_path())
        .unwrap()
        .query_row(
            "SELECT transcript_path FROM sessions WHERE agent_session = 's-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored.as_deref(), Some(path));
}

/// One step of a sequence: the event, the second its hook fired, and what
/// the report adds.
type Step = (&'static str, i64, Value);

/// The state, the turn, whether the session finished, and its attention
/// that a sequence of reports should leave.
type Expected = (&'static str, i64, bool, &'static str);

/// The state, the turn, whether the session finished, and its attention
/// after a sequence of reports.
async fn after(steps: Vec<Step>) -> (String, i64, bool, String) {
    let app = app();
    for (event, s, extra) in steps {
        report(&app, "s-1", event, s, extra).await;
    }
    // a status shows the session, so the list carries its attention
    call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        Some(json!({"text": "Working"})),
    )
    .await;
    let presence = db(&app)
        .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .unwrap();
    let (_, listing) = call(&app, "GET", "/api/v1/sessions", None).await;
    let attention = listing["sessions"][0]["attention"]
        .as_str()
        .unwrap()
        .to_string();
    // the list's attention, worked out in SQL, is the presence service's
    let derived = pinrail_core::presence::attention(Some(&presence), false, chrono::Utc::now());
    assert_eq!(attention, derived.as_str());
    (
        presence.state,
        presence.turn,
        presence.finished_at.is_some(),
        attention,
    )
}

#[tokio::test]
async fn each_kind_of_report_leaves_the_session_in_its_state_and_attention() {
    let process = |pid: u32| json!({"pid": pid, "started": "x"});
    let me = std::process::id();
    let cases: Vec<(&str, Vec<Step>, Expected)> = vec![
        (
            "a finished turn",
            vec![
                ("prompt", 1, json!({})),
                ("turn_ended", 2, json!({"end": "done"})),
            ],
            ("idle", 1, true, "finished"),
        ),
        (
            "an interrupt, as Claude Code's idle prompt reports it",
            vec![
                ("prompt", 1, json!({})),
                ("turn_ended", 2, json!({"end": "interrupted"})),
            ],
            ("idle", 1, false, "idle"),
        ),
        (
            "a permission the agent waits on",
            vec![
                ("prompt", 1, json!({})),
                ("needs_input", 2, json!({"wait": "permission"})),
            ],
            ("needs_input", 1, false, "needs_you"),
        ),
        (
            "a compaction while working",
            vec![
                ("prompt", 1, json!({})),
                ("started", 2, json!({"source": "compact"})),
            ],
            ("working", 1, false, "working"),
        ),
        (
            "a turn end with no open turn",
            vec![
                ("started", 1, json!({})),
                ("turn_ended", 2, json!({"end": "done"})),
            ],
            ("idle", 0, true, "finished"),
        ),
        (
            "an end from a second process while the first runs",
            vec![
                (
                    "prompt",
                    1,
                    json!({"process": {"pid": me, "started": pinrail_runtime_start(me)}}),
                ),
                ("ended", 2, json!({"process": process(1)})),
            ],
            ("working", 1, false, "working"),
        ),
        (
            "the session's own end",
            vec![
                ("prompt", 1, json!({"process": process(me)})),
                ("ended", 2, json!({"process": process(me)})),
            ],
            ("stopped", 1, false, "stopped"),
        ),
    ];
    for (name, steps, (state, turn, finished, attention)) in cases {
        let got = after(steps).await;
        assert_eq!(
            got,
            (state.to_string(), turn, finished, attention.to_string()),
            "{name}"
        );
    }
}

/// This process's start time, as the core reads it.
fn pinrail_runtime_start(pid: u32) -> String {
    pinrail_runtime::process::process(pid).unwrap().started
}

#[tokio::test]
async fn a_session_seen_while_it_works_is_finished_when_the_turn_ends() {
    let app = app();
    call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        Some(json!({"text": "Go"})),
    )
    .await;
    report(&app, "s-1", "prompt", 1, json!({})).await;
    let (code, _) = call(
        &app,
        "PUT",
        &format!(
            "/api/v1/sessions/claude-code/{}/seen",
            common::sid(&app, "claude-code", "s-1")
        ),
        None,
    )
    .await;
    assert_eq!(code, StatusCode::NO_CONTENT);
    report(&app, "s-1", "turn_ended", 2, json!({"end": "done"})).await;
    let listed = |listing: &Value| {
        listing["sessions"][0]["attention"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (_, listing) = call(&app, "GET", "/api/v1/sessions", None).await;
    assert_eq!(listed(&listing), "finished");
    // seen now: idle; marked unread: finished again
    call(
        &app,
        "PUT",
        &format!(
            "/api/v1/sessions/claude-code/{}/seen",
            common::sid(&app, "claude-code", "s-1")
        ),
        None,
    )
    .await;
    let (_, listing) = call(&app, "GET", "/api/v1/sessions", None).await;
    assert_eq!(listed(&listing), "idle");
    call(
        &app,
        "DELETE",
        &format!(
            "/api/v1/sessions/claude-code/{}/seen",
            common::sid(&app, "claude-code", "s-1")
        ),
        None,
    )
    .await;
    let (_, listing) = call(&app, "GET", "/api/v1/sessions", None).await;
    assert_eq!(listed(&listing), "finished");
    assert_eq!(listing["sessions"][0]["presence"]["seen_at"], Value::Null);
    // a session no hook reported has nothing to be seen
    let (code, _) = call(
        &app,
        "PUT",
        &format!(
            "/api/v1/sessions/claude-code/{}/seen",
            common::sid(&app, "claude-code", "other")
        ),
        None,
    )
    .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_list_filters_and_sorts_sessions_by_the_attention_they_ask() {
    let app = app();
    for id in ["idle", "working", "finished", "waiting", "quiet"] {
        let path = format!("/api/v1/sessions/claude-code/{id}/status");
        call(&app, "POST", &path, Some(json!({"text": "Go"}))).await;
    }
    report(&app, "idle", "started", 1, json!({})).await;
    report(&app, "working", "prompt", 1, json!({})).await;
    report(&app, "finished", "prompt", 1, json!({})).await;
    report(&app, "finished", "turn_ended", 2, json!({"end": "done"})).await;
    report(
        &app,
        "waiting",
        "needs_input",
        1,
        json!({"wait": "question"}),
    )
    .await;
    let ids = |listing: &Value| -> Vec<String> {
        listing["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["agent_session"].as_str().unwrap().to_string())
            .collect()
    };
    let (_, listing) = call(&app, "GET", "/api/v1/sessions?sort=priority", None).await;
    assert_eq!(
        ids(&listing),
        ["waiting", "finished", "working", "idle", "quiet"]
    );
    let (_, listing) = call(
        &app,
        "GET",
        "/api/v1/sessions?attention=needs_you,finished",
        None,
    )
    .await;
    let mut got = ids(&listing);
    got.sort();
    assert_eq!(got, ["finished", "waiting"]);
    assert_eq!(listing["total"], 2);
    let (code, _) = call(&app, "GET", "/api/v1/sessions?attention=sleepy", None).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    let (code, _) = call(&app, "GET", "/api/v1/sessions?sort=loudest", None).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn presence_and_turns_are_announced_only_when_they_change() {
    let app = app();
    report(&app, "s-1", "prompt", 1, json!({})).await;
    for s in 2..6 {
        report(&app, "s-1", "working", s, json!({})).await;
    }
    report(&app, "s-1", "turn_ended", 6, json!({"end": "done"})).await;
    call(
        &app,
        "PUT",
        &format!(
            "/api/v1/sessions/claude-code/{}/seen",
            common::sid(&app, "claude-code", "s-1")
        ),
        None,
    )
    .await;
    let conn = rusqlite::Connection::open(app.state.config().db_path()).unwrap();
    let kinds: Vec<String> = conn
        .prepare("SELECT kind FROM events WHERE kind LIKE 'session_%' ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        kinds,
        [
            // the prompt: working, and turn 1 opened
            "session_presence",
            "session_turn",
            // the repeated reports announce nothing; the turn end does
            "session_presence",
            "session_turn",
            "session_seen",
        ]
    );
}

#[tokio::test]
async fn a_session_that_works_and_reports_nothing_for_half_an_hour_is_not_known() {
    let app = app();
    call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        Some(json!({"text": "Go"})),
    )
    .await;
    report(&app, "s-1", "prompt", 1, json!({})).await;
    let old = (chrono::Utc::now() - chrono::Duration::minutes(31))
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    rusqlite::Connection::open(app.state.config().db_path())
        .unwrap()
        .execute("UPDATE session_presence SET updated_at = ?1", [&old])
        .unwrap();
    let (_, listing) = call(&app, "GET", "/api/v1/sessions", None).await;
    assert_eq!(listing["sessions"][0]["attention"], "unknown");
    assert_eq!(listing["sessions"][0]["presence"]["updated_at"], old);
    let presence = db(&app)
        .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .unwrap();
    let derived = pinrail_core::presence::attention(Some(&presence), false, chrono::Utc::now());
    assert_eq!(derived.as_str(), "unknown");
}

#[tokio::test]
async fn a_question_waiting_on_the_person_needs_them_whatever_the_agent_does() {
    let app = app();
    report(&app, "s-1", "prompt", 1, json!({})).await;
    let question = json!({
        "kind": "question",
        "title": "Deploy?",
        "requested_by": "claude-code",
        "session": "s-1",
        "payload": {"questions": [{"id": "q1", "question": "Deploy?", "answer": "yes-no"}]}
    });
    let (code, _) = call(&app, "POST", "/api/v1/reviews", Some(question)).await;
    assert_eq!(code, StatusCode::CREATED);
    let (_, listing) = call(&app, "GET", "/api/v1/sessions", None).await;
    assert_eq!(listing["sessions"][0]["attention"], "needs_you");
    assert_eq!(listing["sessions"][0]["presence"]["state"], "working");
    // the next change the agent reports carries it too
    report(&app, "s-1", "turn_ended", 2, json!({"end": "done"})).await;
    let attrs: String = rusqlite::Connection::open(app.state.config().db_path())
        .unwrap()
        .query_row(
            "SELECT attrs FROM events WHERE kind = 'session_presence' ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let attrs: Value = serde_json::from_str(&attrs).unwrap();
    assert_eq!(
        (attrs["agent"].as_str(), attrs["id"].as_str()),
        (
            Some("claude-code"),
            Some(common::sid(&app, "claude-code", "s-1").as_str())
        )
    );
    assert_eq!(attrs["presence"]["state"], "idle");
    assert_eq!(attrs["presence"]["attention"], "needs_you");
}

#[tokio::test]
async fn a_session_whose_hooks_never_reported_has_nothing_to_see() {
    let app = app();
    call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        Some(json!({"text": "Go"})),
    )
    .await;
    for method in ["PUT", "DELETE"] {
        let (code, _) = call(
            &app,
            method,
            &format!(
                "/api/v1/sessions/claude-code/{}/seen",
                common::sid(&app, "claude-code", "s-1")
            ),
            None,
        )
        .await;
        assert_eq!(code, StatusCode::NO_CONTENT, "{method}");
    }
}

#[tokio::test]
async fn a_new_wait_is_announced_and_a_repeated_one_is_not() {
    let app = app();
    let announced = |app: &App| -> i64 {
        rusqlite::Connection::open(app.state.config().db_path())
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM events WHERE kind = 'session_presence'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };
    let wait = |tool: &str, summary: &str| json!({"wait": "permission", "detail": {"tool": tool, "summary": summary}});
    report(&app, "s-1", "needs_input", 1, wait("Bash", "npm test")).await;
    let first = announced(&app);
    // another call waits, of the same kind
    report(&app, "s-1", "needs_input", 2, wait("Edit", "src/app.rs")).await;
    assert_eq!(announced(&app), first + 1);
    // the same wait again, as Claude Code's notification repeats it
    report(&app, "s-1", "needs_input", 3, json!({"wait": "permission"})).await;
    assert_eq!(announced(&app), first + 1);
}

/// A request as an agent sends it, from outside the app's window, naming
/// itself and its session in the body.
async fn as_agent(app: &App, method: &str, path: &str, session: &str, body: Value) -> StatusCode {
    let mut body = body;
    body["by"] = json!("claude-code");
    body["session"] = json!(session);
    let request = axum::http::Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:4747")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap();
    use tower::ServiceExt;
    app.router.clone().oneshot(request).await.unwrap().status()
}

#[tokio::test]
async fn a_hidden_session_shows_once_it_needs_the_person_is_messaged_or_works_on_an_item() {
    let app = app();
    let shown = |app: &App, id: &str| -> bool {
        let hidden: i64 = rusqlite::Connection::open(app.state.config().db_path())
            .unwrap()
            .query_row(
                "SELECT hidden FROM sessions WHERE agent = 'claude-code' AND agent_session = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        hidden == 0
    };
    for id in ["waits", "messaged", "adds", "comments"] {
        report(&app, id, "prompt", 1, json!({})).await;
        assert!(!shown(&app, id), "{id}");
    }
    // it needs the person
    report(
        &app,
        "waits",
        "needs_input",
        2,
        json!({"wait": "permission"}),
    )
    .await;
    assert!(shown(&app, "waits"));
    // the person sends it a message, from a notification's thread
    let (code, _) = call(
        &app,
        "POST",
        &format!(
            "/api/v1/sessions/claude-code/{}/messages",
            common::sid(&app, "claude-code", "messaged")
        ),
        Some(json!({"text": "Use the staging database"})),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED);
    assert!(shown(&app, "messaged"));
    // it adds a work item, or comments on one
    let created = as_agent(
        &app,
        "POST",
        "/api/v1/items",
        "adds",
        json!({"repo": "acme/web", "title": "Fix login"}),
    )
    .await;
    assert_eq!(created, StatusCode::CREATED);
    assert!(shown(&app, "adds"));
    let commented = as_agent(
        &app,
        "POST",
        "/api/v1/items/WEB-1/comments",
        "comments",
        json!({"body": "Started on it"}),
    )
    .await;
    assert_eq!(commented, StatusCode::CREATED);
    assert!(shown(&app, "comments"));
}

/// A home folder of its own, with Claude Code's status file for this
/// process saying `status` about `session` since `since_ms`.
fn home_saying(name: &str, session: &str, status: &str, since_ms: i64) -> std::path::PathBuf {
    let home = std::env::temp_dir().join(format!("pinrail-check-{name}-{}", std::process::id()));
    let sessions = home.join(".claude/sessions");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(
        sessions.join(format!("{}.json", std::process::id())),
        json!({"sessionId": session, "status": status, "statusUpdatedAt": since_ms}).to_string(),
    )
    .unwrap();
    home
}

/// This process, as a hook reports the agent's.
fn this_process() -> Value {
    let me = std::process::id();
    json!({"pid": me, "started": pinrail_runtime::process::process(me).unwrap().started})
}

#[tokio::test]
async fn an_interrupted_claude_code_turn_ends_when_its_status_file_says_idle() {
    let app = app();
    report(&app, "s-1", "prompt", 1, json!({"process": this_process()})).await;
    let later = chrono::Utc::now() + chrono::Duration::seconds(1);
    let home = home_saying("idle", "s-1", "idle", later.timestamp_millis());
    app.state.reviews().check_presence(&home, later).unwrap();
    let presence = db(&app)
        .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .unwrap();
    assert_eq!(presence.state, "idle");
    assert_eq!(presence.finished_at, None);
    let turn = db(&app)
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(turn.end_kind.as_deref(), Some("interrupted"));
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn a_session_that_still_works_or_whose_file_is_older_is_left_working() {
    let app = app();
    report(
        &app,
        "busy",
        "prompt",
        1,
        json!({"process": this_process()}),
    )
    .await;
    report(
        &app,
        "stale",
        "prompt",
        1,
        json!({"process": this_process()}),
    )
    .await;
    let now = chrono::Utc::now();
    // busy, and idle from before the turn began
    let busy = home_saying("busy", "busy", "busy", now.timestamp_millis());
    app.state.reviews().check_presence(&busy, now).unwrap();
    let stale = home_saying(
        "stale",
        "stale",
        "idle",
        (now - chrono::Duration::hours(1)).timestamp_millis(),
    );
    app.state.reviews().check_presence(&stale, now).unwrap();
    for id in ["busy", "stale"] {
        assert_eq!(
            db(&app)
                .presence("claude-code", &common::sid(&app, "claude-code", id))
                .unwrap()
                .unwrap()
                .state,
            "working",
            "{id}"
        );
    }
    let _ = std::fs::remove_dir_all(busy);
    let _ = std::fs::remove_dir_all(stale);
}

#[tokio::test]
async fn a_session_whose_agent_process_is_gone_has_stopped() {
    let app = app();
    // a process that ran once: this pid with another start
    let gone = json!({"pid": std::process::id(), "started": "long ago"});
    report(
        &app,
        "s-1",
        "needs_input",
        1,
        json!({"wait": "permission", "process": gone}),
    )
    .await;
    let nowhere = std::env::temp_dir().join("pinrail-check-nowhere");
    app.state
        .reviews()
        .check_presence(&nowhere, chrono::Utc::now())
        .unwrap();
    let presence = db(&app)
        .presence("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap()
        .unwrap();
    assert_eq!(
        (presence.state.as_str(), presence.stop_reason.as_deref()),
        ("stopped", Some("exited"))
    );
}

#[tokio::test]
async fn a_turn_keeps_its_reply_whichever_of_the_hook_and_the_terminal_ends_it_last() {
    use pinrail_core::db::{ReportEvent, Source};
    let app = app();
    let db = db(&app);
    let report = |event, source| pinrail_core::db::Report {
        event,
        at: chrono::Utc::now(),
        process: None,
        origin: Default::default(),
        transcript_path: None,
        agent_turn: None,
        cut: false,
        source,
    };
    let prompt = || ReportEvent::Prompt {
        prompt: Some("fix the login".into()),
    };
    let stop = || ReportEvent::TurnEnded {
        end: "done".into(),
        reply: Some("Fixed the redirect.".into()),
    };
    let done = || ReportEvent::TurnEnded {
        end: "done".into(),
        reply: None,
    };
    let ends = |session: &str| {
        db.turns("claude-code", &common::sid(&app, "claude-code", session))
            .unwrap()
            .into_iter()
            .map(|t| (t.end_kind.clone(), t.reply().map(str::to_string)))
            .collect::<Vec<_>>()
    };
    let replied = vec![(
        Some("done".to_string()),
        Some("Fixed the redirect.".to_string()),
    )];
    let now = chrono::Utc::now;

    // the hook's end, then the terminal's: the reply stays
    db.record(
        "claude-code",
        "s-hook-first",
        &report(ReportEvent::Working, Source::Terminal),
        true,
        now(),
    )
    .unwrap();
    db.record(
        "claude-code",
        "s-hook-first",
        &report(prompt(), Source::Hook),
        true,
        now(),
    )
    .unwrap();
    db.record(
        "claude-code",
        "s-hook-first",
        &report(stop(), Source::Hook),
        true,
        now(),
    )
    .unwrap();
    db.record(
        "claude-code",
        "s-hook-first",
        &report(done(), Source::Terminal),
        true,
        now(),
    )
    .unwrap();
    assert_eq!(ends("s-hook-first"), replied);

    // the terminal's end, then the hook's: the hook brings the reply
    db.record(
        "claude-code",
        "s-hook-last",
        &report(ReportEvent::Working, Source::Terminal),
        true,
        now(),
    )
    .unwrap();
    db.record(
        "claude-code",
        "s-hook-last",
        &report(prompt(), Source::Hook),
        true,
        now(),
    )
    .unwrap();
    db.record(
        "claude-code",
        "s-hook-last",
        &report(done(), Source::Terminal),
        true,
        now(),
    )
    .unwrap();
    db.record(
        "claude-code",
        "s-hook-last",
        &report(stop(), Source::Hook),
        true,
        now(),
    )
    .unwrap();
    assert_eq!(ends("s-hook-last"), replied);

    // with no hook, the terminal's end closes the open turn
    db.record(
        "claude-code",
        "s-terminal",
        &report(prompt(), Source::Hook),
        true,
        now(),
    )
    .unwrap();
    db.record(
        "claude-code",
        "s-terminal",
        &report(done(), Source::Terminal),
        true,
        now(),
    )
    .unwrap();
    assert_eq!(ends("s-terminal"), vec![(Some("done".to_string()), None)]);
}

#[tokio::test]
async fn an_interrupt_and_a_failure_of_one_turn_end_it_as_interrupted() {
    // Cursor reports Ctrl+C as two ends of the turn, in either order
    let app = app();
    let ends = |app: &App, session: &str| {
        db(app)
            .turns("claude-code", &common::sid(app, "claude-code", session))
            .unwrap()
            .into_iter()
            .map(|t| t.end_kind)
            .collect::<Vec<_>>()
    };
    let turn = json!({"detail": {"turn_id": "g-1"}});
    for (session, first, second) in [
        ("s-1", "interrupted", "failed"),
        ("s-2", "failed", "interrupted"),
    ] {
        report(&app, session, "prompt", 1, turn.clone()).await;
        let mut end = turn.clone();
        end["end"] = json!(first);
        report(&app, session, "turn_ended", 2, end.clone()).await;
        end["end"] = json!(second);
        report(&app, session, "turn_ended", 2, end).await;
        assert_eq!(
            ends(&app, session),
            vec![Some("interrupted".to_string())],
            "{session}"
        );
    }
    // a failure alone stays one
    report(&app, "s-3", "prompt", 1, turn.clone()).await;
    let mut failed = turn.clone();
    failed["end"] = json!("failed");
    report(&app, "s-3", "turn_ended", 2, failed).await;
    assert_eq!(ends(&app, "s-3"), vec![Some("failed".to_string())]);
}

#[tokio::test]
async fn a_turn_that_goes_on_after_its_reply_keeps_both_replies() {
    let app = app();
    report(
        &app,
        "s-1",
        "prompt",
        1,
        json!({"detail": {"prompt": "Commit it", "turn_id": "p-1"}}),
    )
    .await;
    report(
        &app,
        "s-1",
        "turn_ended",
        2,
        json!({"end": "done", "detail": {"reply": "Committed. Marking PM-2 done has not gone through yet.", "turn_id": "p-1"}}),
    )
    .await;
    // a background job ends, and the agent replies again in the same turn
    report(
        &app,
        "s-1",
        "turn_ended",
        3,
        json!({"end": "done", "detail": {"reply": "PM-2 is now marked done.", "turn_id": "p-1"}}),
    )
    .await;
    let turns = db(&app)
        .turns("claude-code", &common::sid(&app, "claude-code", "s-1"))
        .unwrap();
    assert_eq!(turns.len(), 1);
    // two messages, each with its own time
    let replies: Vec<&str> = turns[0].replies.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(
        replies,
        [
            "Committed. Marking PM-2 done has not gone through yet.",
            "PM-2 is now marked done."
        ]
    );
    assert!(turns[0].replies[0].at < turns[0].replies[1].at);
}
