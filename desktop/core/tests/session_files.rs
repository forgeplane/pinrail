//! `/api/v1/sessions/{agent}/{id}/files/{name}`: the files the person attaches to
//! a session's prompts, saved where the agent can open them by path.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

mod common;
use common::{App, app, call, call_as};

/// A session the agent reported, and its path in the API.
async fn session(app: &App) -> String {
    let body = json!({"text": "Working", "by": "claude-code"});
    let (code, answer) = call_as(
        app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        Some(body),
        &[],
    )
    .await;
    assert!(code.is_success(), "{code} {answer}");
    format!(
        "/api/v1/sessions/claude-code/{}",
        common::sid(app, "claude-code", "s-1")
    )
}

/// Saves `bytes` as `name` for the session at `path`, as the window sends
/// it, with the person token unless `person` is false.
async fn save(
    app: &App,
    path: &str,
    name: &str,
    bytes: &[u8],
    person: bool,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("PUT")
        .uri(format!(
            "{path}/files/{}",
            name.replace(' ', "%20").replace('/', "%2F")
        ))
        .header("host", "127.0.0.1:4747")
        .header("content-type", "application/octet-stream");
    if person {
        request = request.header("origin", "tauri://localhost").header(
            "authorization",
            format!("Bearer {}", app.state.config().person_token),
        );
    }
    let request = request.body(Body::from(bytes.to_vec())).unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn a_saved_file_keeps_its_name_and_a_taken_name_gets_a_number() {
    let app = app();
    let path = session(&app).await;
    let (code, first) = save(&app, &path, "screen shot.png", b"one", true).await;
    assert_eq!(code, StatusCode::CREATED, "{first}");
    let (code, second) = save(&app, &path, "screen shot.png", b"two", true).await;
    assert_eq!(code, StatusCode::CREATED, "{second}");

    let first = std::path::PathBuf::from(first["path"].as_str().unwrap());
    let second = std::path::PathBuf::from(second["path"].as_str().unwrap());
    assert!(first.is_absolute());
    assert_eq!(first.file_name().unwrap(), "screen-shot.png");
    assert_eq!(second.file_name().unwrap(), "screen-shot-2.png");
    assert_eq!(first.parent(), second.parent());
    assert!(first.starts_with(app.dir.path()));
    assert_eq!(std::fs::read(&first).unwrap(), b"one");
    assert_eq!(std::fs::read(&second).unwrap(), b"two");
}

#[tokio::test]
async fn a_name_cannot_leave_the_sessions_folder() {
    let app = app();
    let path = session(&app).await;
    let (code, saved) = save(&app, &path, "../../../escape.txt", b"x", true).await;
    assert_eq!(code, StatusCode::CREATED, "{saved}");
    let saved = std::path::PathBuf::from(saved["path"].as_str().unwrap());
    assert_eq!(saved.file_name().unwrap(), "escape.txt");
    assert!(saved.parent().unwrap().ends_with("files"));
}

#[tokio::test]
async fn only_the_person_saves_files_and_only_for_a_known_session() {
    let app = app();
    let path = session(&app).await;
    let (code, _) = save(&app, &path, "a.txt", b"x", false).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    let (code, _) = save(
        &app,
        "/api/v1/sessions/claude-code/nope",
        "a.txt",
        b"x",
        true,
    )
    .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_the_session_deletes_its_files() {
    let app = app();
    let path = session(&app).await;
    let (_, saved) = save(&app, &path, "notes.md", b"# Notes", true).await;
    let saved = std::path::PathBuf::from(saved["path"].as_str().unwrap());
    assert!(saved.exists());

    let (code, answer) = call(&app, "DELETE", &path, None).await;
    assert_eq!(code, StatusCode::NO_CONTENT, "{answer}");
    assert!(!saved.parent().unwrap().exists());
}

#[tokio::test]
async fn the_sweep_deletes_the_files_of_sessions_that_are_gone() {
    let app = app();
    let path = session(&app).await;
    let (_, saved) = save(&app, &path, "notes.md", b"# Notes", true).await;
    let saved = std::path::PathBuf::from(saved["path"].as_str().unwrap());
    // the session goes some other way than the person deleting it
    common::db(&app)
        .delete_session(
            "claude-code",
            &common::sid(&app, "claude-code", "s-1"),
            None,
            chrono::Utc::now(),
        )
        .unwrap();
    assert!(saved.exists());

    app.state.session_files().sweep().unwrap();
    assert!(!saved.parent().unwrap().exists());
}

#[tokio::test]
async fn a_saved_file_is_read_back_by_its_name_and_nothing_else_is() {
    let app = app();
    let path = session(&app).await;
    let (_, saved) = save(&app, &path, "shot.png", b"PNG bytes", true).await;
    let name = saved["name"].as_str().unwrap();

    let (code, body) = get(&app, &format!("{path}/files/{name}")).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body, b"PNG bytes");
    for missing in ["other.png", "..%2F..%2Fpinrail.db", ".."] {
        let (code, _) = get(&app, &format!("{path}/files/{missing}")).await;
        assert_eq!(code, StatusCode::NOT_FOUND, "{missing}");
    }
}

/// A GET as any program on this machine sends it, and the body's bytes.
async fn get(app: &App, uri: &str) -> (StatusCode, Vec<u8>) {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .header("host", "127.0.0.1:4747")
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, bytes.to_vec())
}
