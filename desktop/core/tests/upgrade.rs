//! A database an earlier release left, opened by this build: every
//! migration since runs, and what the person had is still there.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;
use pinrail_core::api::router;
use pinrail_core::{Config, Pinrail};
use serde_json::Value;

/// A core on a copy of the database 0.6.0 wrote.
fn from_0_6_0() -> (tempfile::TempDir, common::App) {
    from_0_6_0_with("")
}

/// A core on a copy of the database 0.6.0 wrote, changed by `sql` first.
fn from_0_6_0_with(sql: &str) -> (tempfile::TempDir, common::App) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/db/pinrail-0.6.0.db"
        ),
        dir.path().join("pinrail.db"),
    )
    .unwrap();
    if !sql.is_empty() {
        let conn = rusqlite::Connection::open(dir.path().join("pinrail.db")).unwrap();
        conn.execute_batch(sql).unwrap();
    }
    let mut config = Config::new(dir.path(), 0);
    config.telemetry_env = false;
    let state = Arc::new(Pinrail::open(config).unwrap());
    let app = common::App {
        router: router(state.clone()),
        state,
        dir: tempfile::tempdir().unwrap(),
    };
    (dir, app)
}

#[tokio::test]
async fn a_database_from_0_6_0_keeps_its_sessions_reviews_and_items() {
    let (_dir, app) = from_0_6_0();
    let (_, sessions) =
        common::call(&app, "GET", "/api/v1/sessions?include_archived=true", None).await;
    let ids: Vec<&str> = sessions["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["agent_session"].as_str().unwrap())
        .collect();
    assert_eq!(sessions["total"], 2, "{sessions}");
    assert!(ids.contains(&"s-060") && ids.contains(&"s-old"));
    // each has an id of Pinrail's own, and keeps the agent's beside it
    for session in sessions["sessions"].as_array().unwrap() {
        assert!(
            session["id"].as_str().unwrap().starts_with("ses_"),
            "{session}"
        );
    }
    // no hook ever reported for them: no presence, shown, the question
    // still waiting on the person
    let session = sessions["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["agent_session"] == "s-060")
        .unwrap();
    let id = session["id"].as_str().unwrap().to_string();
    assert_eq!(session["presence"], Value::Null);
    assert_eq!(session["attention"], "needs_you");
    assert_eq!(session["title"], "Add rate limiting");

    let (code, thread) = common::call(
        &app,
        "GET",
        &format!("/api/v1/sessions/claude-code/{id}"),
        None,
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(thread["statuses"].as_array().unwrap().len(), 3);
    assert_eq!(thread["messages"].as_array().unwrap().len(), 1);
    assert_eq!(thread["turns"], serde_json::json!([]));

    let (_, reviews) =
        common::call(&app, "GET", "/api/v1/reviews?status=pending,decided", None).await;
    assert_eq!(reviews["reviews"].as_array().unwrap().len(), 2, "{reviews}");
    // the reviews name their sessions by Pinrail's ids
    for review in reviews["reviews"].as_array().unwrap() {
        if !review["session"].is_null() {
            assert!(
                review["session"].as_str().unwrap().starts_with("ses_"),
                "{review}"
            );
        }
    }

    let (_, items) = common::call(
        &app,
        "GET",
        "/api/v1/items?status=open,in_progress,on_hold,done,cancelled",
        None,
    )
    .await;
    let items = items["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let claimed = items
        .iter()
        .find(|i| i["title"] == "Limit the login route")
        .unwrap();
    // claimed by the person, from the app's window, and in progress since
    assert_eq!(claimed["status"], "in_progress", "{claimed}");

    // the presence tables are there and work on the old sessions
    let (code, _) = common::call(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-060/reports",
        Some(serde_json::json!({"event": "prompt"})),
    )
    .await;
    assert_eq!(code, StatusCode::NO_CONTENT);
    let (_, thread) = common::call(
        &app,
        "GET",
        &format!("/api/v1/sessions/claude-code/{id}"),
        None,
    )
    .await;
    assert_eq!(
        thread["session"]["presence"]["state"], "working",
        "{thread}"
    );
    let conn = rusqlite::Connection::open(app.state.config().db_path()).unwrap();
    let broken: Vec<String> = conn
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(broken.is_empty(), "{broken:?}");
}

#[test]
fn a_database_from_0_6_0_takes_runs_for_its_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pinrail.db");
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/db/pinrail-0.6.0.db"
        ),
        &path,
    )
    .unwrap();
    let db = pinrail_core::db::Db::open(&path).unwrap();
    let session = db.find_session("claude-code", "s-060").unwrap().unwrap();
    let run = db
        .insert_run(
            &pinrail_core::db::NewRun {
                id: "run_1",
                host: "local",
                agent: "claude-code",
                kind: "resume",
                folder: "/work/api",
                argv: &["claude".to_string()],
                task: None,
                args: &[],
                created_by: "app",
                token_digest: "d1",
                session: Some(&session),
                agent_session: None,
                checkout: None,
            },
            chrono::Utc::now(),
        )
        .unwrap();
    assert_eq!(run.host, "local");
    assert_eq!(run.session, Some(("claude-code".to_string(), session)));
    assert_eq!(run.agent_session.as_deref(), Some("s-060"));
}

#[tokio::test]
async fn the_sessions_of_an_earlier_release_are_named_when_it_opens() {
    let (_dir, app) = from_0_6_0();
    let (_, sessions) =
        common::call(&app, "GET", "/api/v1/sessions?include_archived=true", None).await;
    for session in sessions["sessions"].as_array().unwrap() {
        let number = session["element"].as_u64().expect("an element") as u32;
        let element = pinrail_core::elements::by_number(number).expect("a known element");
        let name = session["generated_name"].as_str().expect("a name");
        assert!(name.ends_with(&format!(" {}", element.name)), "{name}");
        // no name of the person's
        assert!(session["name"].is_null());
        // the stage title stays as it was
        if session["agent_session"] == "s-060" {
            assert_eq!(session["title"], "Add rate limiting");
        }
    }
}

#[tokio::test]
async fn a_claim_of_a_session_known_only_from_its_items_outlives_the_hidden_session_sweep() {
    // an agent's session that only worked on an item, claimed long ago
    let (dir, app) = from_0_6_0_with(
        "UPDATE work_items SET status = 'in_progress', claim_agent = 'claude-code',
           claim_session = 's-item', updated_at = '2026-11-01T00:00:00Z'
         WHERE number = 1",
    );
    let conn = rusqlite::Connection::open(dir.path().join("pinrail.db")).unwrap();
    let held = || -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM work_items w JOIN sessions s ON s.id = w.claim_session
              WHERE w.number = 1 AND s.agent_session = 's-item'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(held(), 1, "the upgrade names the claim's session");
    let later = chrono::Utc::now() + chrono::Duration::seconds(1);
    common::db(&app).delete_hidden_sessions(later).unwrap();
    assert_eq!(held(), 1, "the claim names a session that is gone");
}
