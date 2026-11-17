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
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/db/pinrail-0.6.0.db"
        ),
        dir.path().join("pinrail.db"),
    )
    .unwrap();
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
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(sessions["total"], 2, "{sessions}");
    assert!(ids.contains(&"s-060") && ids.contains(&"s-old"));
    // no hook ever reported for them: no presence, shown, the question
    // still waiting on the person
    let session = sessions["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "s-060")
        .unwrap();
    assert_eq!(session["presence"], Value::Null);
    assert_eq!(session["attention"], "needs_you");
    assert_eq!(session["title"], "Add rate limiting");

    let (code, thread) =
        common::call(&app, "GET", "/api/v1/sessions/claude-code/s-060", None).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(thread["statuses"].as_array().unwrap().len(), 3);
    assert_eq!(thread["messages"].as_array().unwrap().len(), 1);
    assert_eq!(thread["turns"], serde_json::json!([]));

    let (_, reviews) =
        common::call(&app, "GET", "/api/v1/reviews?status=pending,decided", None).await;
    assert_eq!(reviews["reviews"].as_array().unwrap().len(), 2, "{reviews}");

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
                session: Some("s-060"),
            },
            chrono::Utc::now(),
        )
        .unwrap();
    assert_eq!(run.host, "local");
    assert_eq!(
        run.session,
        Some(("claude-code".to_string(), "s-060".to_string()))
    );
}

#[tokio::test]
async fn the_sessions_of_an_earlier_release_are_named_when_it_opens() {
    let (_dir, app) = from_0_6_0();
    let (_, sessions) =
        common::call(&app, "GET", "/api/v1/sessions?include_archived=true", None).await;
    for session in sessions["sessions"].as_array().unwrap() {
        let number = session["element"].as_u64().expect("an element") as u32;
        let element = pinrail_core::elements::by_number(number).expect("a known element");
        let name = session["name"].as_str().expect("a name");
        assert!(name.ends_with(&format!(" {}", element.name)), "{name}");
        // the stage title stays as it was
        if session["id"] == "s-060" {
            assert_eq!(session["title"], "Add rate limiting");
        }
    }
}
