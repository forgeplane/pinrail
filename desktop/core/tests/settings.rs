//! `/api/v1/settings` against a fresh data directory: defaults, a change,
//! a refusal, an edit to the file, and the port read at start.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use wicket_core::Config;
use wicket_core::{Wicket, api};

struct App {
    dir: tempfile::TempDir,
    state: Arc<Wicket>,
    router: Router,
}

fn app() -> App {
    let dir = tempfile::tempdir().unwrap();
    let state = Wicket::open(Config::new(dir.path(), 0)).unwrap();
    let router = api::router(state.clone());
    App { dir, state, router }
}

async fn call(app: &App, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(match body {
            Some(b) => Body::from(b.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

#[tokio::test]
async fn every_setting_is_listed_at_its_default() {
    let app = app();
    let (status, body) = call(&app, "GET", "/api/v1/settings", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["appearance"]["theme"], "system");
    assert_eq!(body["appearance"]["text_size"], "default");
    assert_eq!(body["autostart"], false);
    assert_eq!(body["close_window"], "hide");
    assert_eq!(body["menu_bar_icon"], true);
    assert_eq!(body["sidebar"]["open"], true);
    assert_eq!(body["notifications"]["enabled"], true);
    assert!(body["notifications"]["paused_until"].is_null());
    assert_eq!(body["notifications"]["sound"], true);
    assert_eq!(body["notifications"]["muted_plugins"], json!([]));
    assert!(body["notifications"]["quiet_hours"].is_null());
    assert_eq!(body["shortcut"]["global"], "alt+shift+w");
    assert_eq!(body["shortcut"]["global_opens"], "oldest");
    assert_eq!(body["port"], 4747);
    assert!(body["history"]["keep_days"].is_null());
    assert!(
        !app.dir.path().join("settings.json").exists(),
        "nothing is written until something changes"
    );
}

#[tokio::test]
async fn a_change_is_applied_written_and_announced() {
    let app = app();
    let mut rx = app.state.events().subscribe();
    let (status, body) = call(
        &app,
        "PATCH",
        "/api/v1/settings",
        Some(json!({"appearance": {"theme": "light"}, "notifications": {"muted_plugins": ["hello"]}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["appearance"]["theme"], "light");
    assert_eq!(body["appearance"]["text_size"], "default");
    assert_eq!(body["notifications"]["muted_plugins"], json!(["hello"]));

    let (_, again) = call(&app, "GET", "/api/v1/settings", None).await;
    assert_eq!(again, body, "GET returns what PATCH returned");

    let on_disk: Value = serde_json::from_str(
        &std::fs::read_to_string(app.dir.path().join("settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        on_disk,
        json!({"appearance": {"theme": "light"}, "notifications": {"muted_plugins": ["hello"]}})
    );

    let notice = rx.try_recv().unwrap();
    assert_eq!(notice.kind, "settings_changed");
    assert_eq!(
        notice.keys,
        Some(vec![
            "/appearance/theme".to_string(),
            "/notifications/muted_plugins".to_string()
        ])
    );
    assert!(notice.review_id.is_none());

    // and the event is on record, keys included, for the stream's backlog
    let events = wicket_core::db::Db::open(&app.state.config().db_path())
        .unwrap()
        .events_after(0, 10)
        .unwrap();
    let recorded = events
        .iter()
        .find(|e| e.kind == "settings_changed")
        .unwrap();
    assert_eq!(
        recorded.attrs["keys"],
        json!(["/appearance/theme", "/notifications/muted_plugins"])
    );
}

#[tokio::test]
async fn a_change_that_changes_nothing_announces_nothing() {
    let app = app();
    let mut rx = app.state.events().subscribe();
    let (status, _) = call(
        &app,
        "PATCH",
        "/api/v1/settings",
        Some(json!({"port": 4747})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn a_bad_change_is_refused_with_paths_and_applies_nothing() {
    let app = app();
    let (status, body) = call(
        &app,
        "PATCH",
        "/api/v1/settings",
        Some(json!({"appearance": {"theme": "sepia", "text_size": "large"}, "typo": true})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "invalid");
    assert_eq!(
        body["violations"],
        json!([
            {"path": "/appearance/theme", "message": "must be one of system, dark, light"},
            {"path": "/typo", "message": "unknown setting"}
        ])
    );
    let (_, now) = call(&app, "GET", "/api/v1/settings", None).await;
    assert_eq!(
        now["appearance"]["text_size"], "default",
        "the valid part was not applied either"
    );

    let (status, body) = call(&app, "PATCH", "/api/v1/settings", Some(json!([1]))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["violations"][0]["message"], "must be a JSON object");
}

#[tokio::test]
async fn an_edit_to_the_file_is_picked_up_with_its_keys() {
    let app = app();
    call(
        &app,
        "PATCH",
        "/api/v1/settings",
        Some(json!({"autostart": true})),
    )
    .await;
    assert!(app.state.settings().reload().unwrap().is_none());
    std::fs::write(
        app.dir.path().join("settings.json"),
        r#"{"autostart": true, "shortcut": {"global": "ctrl+alt+r"}, "someday": 1}"#,
    )
    .unwrap();
    let keys = app.state.settings().reload().unwrap().unwrap();
    assert_eq!(keys, vec!["/shortcut/global"]);
    let (_, body) = call(&app, "GET", "/api/v1/settings", None).await;
    assert_eq!(body["shortcut"]["global"], "ctrl+alt+r");
    assert_eq!(body["someday"], 1, "an unknown key passes through");
}

#[test]
fn the_port_in_the_file_is_used_unless_the_environment_says_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("settings.json"), r#"{"port": 4900}"#).unwrap();
    assert_eq!(wicket_core::settings::port_in(dir.path()), Some(4900));
    // Config::from_env reads the environment; the file wins only when
    // WICKET_PORT is unset, which the unit test in config covers through
    // port_in. Here: an absent or invalid file yields nothing.
    std::fs::write(dir.path().join("settings.json"), r#"{"port": "a"}"#).unwrap();
    assert_eq!(wicket_core::settings::port_in(dir.path()), None);
}

#[tokio::test]
async fn the_settings_table_is_gone() {
    let app = app();
    let conn = rusqlite::Connection::open(app.dir.path().join("wicket.db")).unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'settings'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
}

/// A registered plugin with settings of its own, and one without.
async fn with_knobs(app: &App) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let plugin = dir.path().join("knobs");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(plugin.join("index.html"), "<html></html>").unwrap();
    std::fs::write(
        plugin.join("manifest.json"),
        r#"{"name":"knobs","version":1,"title":"Knobs","payload_schema":{},"decision_schema":{},
            "settings_schema":{"type":"object","properties":{
              "diff":{"type":"string","title":"Diff","enum":["inline","split"],"default":"inline"},
              "wrap":{"type":"boolean","title":"Wrap","default":true}}}}"#,
    )
    .unwrap();
    let (status, body) = call(
        app,
        "POST",
        "/api/v1/plugins/dirs",
        Some(json!({"dir": dir.path().display().to_string()})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    dir
}

#[tokio::test]
async fn a_plugin_declares_settings_and_the_core_keeps_them() {
    let app = app();
    let _dir = with_knobs(&app).await;
    let mut rx = app.state.events().subscribe();

    // the row carries the schema and the values as they stand
    let (_, body) = call(&app, "GET", "/api/v1/plugins", None).await;
    let knobs = body["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "knobs")
        .unwrap();
    assert_eq!(
        knobs["settings_schema"]["properties"]["diff"]["title"],
        "Diff"
    );
    assert_eq!(knobs["settings"], json!({"diff": "inline", "wrap": true}));
    let list = body["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "list")
        .unwrap();
    assert!(list["settings_schema"].is_null() && list["settings"].is_null());

    // a change is checked against the plugin's schema and announced leaf by leaf
    let (status, body) = call(
        &app,
        "PATCH",
        "/api/v1/settings",
        Some(json!({"plugins": {"knobs": {"diff": "split"}}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["plugins"], json!({"knobs": {"diff": "split"}}));
    let notice = rx.try_recv().unwrap();
    assert_eq!(notice.keys, Some(vec!["/plugins/knobs/diff".to_string()]));
    let (_, body) = call(&app, "GET", "/api/v1/plugins", None).await;
    let knobs = body["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "knobs")
        .unwrap();
    assert_eq!(knobs["settings"], json!({"diff": "split", "wrap": true}));

    // refused: a value outside the schema, a key it does not have, a plugin without settings
    for (patch, path) in [
        (
            json!({"plugins": {"knobs": {"diff": "wide"}}}),
            "/plugins/knobs/diff",
        ),
        (
            json!({"plugins": {"knobs": {"nope": 1}}}),
            "/plugins/knobs/nope",
        ),
        (
            json!({"plugins": {"list": {"anything": 1}}}),
            "/plugins/list",
        ),
        (
            json!({"plugins": {"knobs": {"wrap": {"deep": true}}}}),
            "/plugins/knobs/wrap",
        ),
    ] {
        let (status, body) = call(&app, "PATCH", "/api/v1/settings", Some(patch.clone())).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{patch}: {body}");
        let paths: Vec<&str> = body["violations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["path"].as_str().unwrap())
            .collect();
        assert!(paths.contains(&path), "{patch}: {body}");
    }
    assert!(rx.try_recv().is_err(), "a refusal announces nothing");

    // a plugin that is not registered now keeps what a newer or older setup wrote
    let (status, body) = call(
        &app,
        "PATCH",
        "/api/v1/settings",
        Some(json!({"plugins": {"gone": {"mode": "x"}}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["plugins"]["gone"], json!({"mode": "x"}));
    assert_eq!(
        rx.try_recv().unwrap().keys,
        Some(vec!["/plugins/gone/mode".to_string()])
    );
}
