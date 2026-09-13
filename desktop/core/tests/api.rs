//! The API against fixtures recorded from the reference server, with the v1
//! renames applied: `type` is `plugin`, `source` is `origin`, `supersedes`
//! is `revises`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use wicket_core::Config;
use wicket_core::api::{AppState, router};

struct App {
    _dir: tempfile::TempDir,
    state: Arc<AppState>,
    router: Router,
}

fn app() -> App {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::new(dir.path(), 0);
    config.user = "tester".into();
    let state = AppState::open(config).unwrap();
    App {
        router: router(state.clone()),
        state,
        _dir: dir,
    }
}

async fn call(app: &App, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:4747")
        .header("content-type", "application/json")
        .body(
            body.map(|b| Body::from(b.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    (status, value)
}

fn fixture(name: &str) -> (u16, Value) {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/elixir/{name}.txt"));
    let text = std::fs::read_to_string(&path).unwrap();
    let (body, status) = text.trim_end().rsplit_once('\n').unwrap();
    (status.parse().unwrap(), serde_json::from_str(body).unwrap())
}

/// The fixture's violations with the v1 names.
fn fixture_violations(name: &str) -> Vec<(String, String)> {
    let (_, body) = fixture(name);
    body["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let path = v["path"]
                .as_str()
                .unwrap()
                .replace("/source", "/origin")
                .replace("/supersedes", "/revises")
                .replace("/type", "/plugin");
            let message = v["message"]
                .as_str()
                .unwrap()
                .replace("unknown gate g_nope", "unknown review r_nope");
            (path, message)
        })
        .collect()
}

fn violations(body: &Value) -> Vec<(String, String)> {
    body["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["path"].as_str().unwrap().into(),
                v["message"].as_str().unwrap().into(),
            )
        })
        .collect()
}

fn list_payload() -> Value {
    json!({
        "intro": "Two proposals from **round 1**.",
        "allow_additions": true,
        "groups": [{
            "title": "lib/acme/tickets.ex",
            "items": [
                {"id": 1, "severity": "major", "title": "do_save dedups without reversing", "body": "Reverse after dedup.", "meta": {"line": 149}},
                {"id": 2, "severity": "minor", "title": "moduledoc typo"}
            ]
        }]
    })
}

fn submission() -> Value {
    json!({
        "plugin": "list",
        "title": "MR !42",
        "origin": {"repo": "acme", "workflow": "review", "ref": "42", "run_id": "r1", "url": "https://x/42", "junk": 1},
        "requested_by": "agent",
        "summary": {"counts": [["major", 1]], "subtitle": "2 items"},
        "payload": list_payload()
    })
}

async fn submit(app: &App, body: Value) -> Value {
    let (status, review) = call(app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    review
}

#[tokio::test]
async fn submit_returns_the_envelope_the_reference_returns() {
    let app = app();
    let review = submit(&app, submission()).await;
    let (_, reference) = fixture("create-ok");

    let mut expected: Vec<&str> = reference
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let renamed: Vec<String> = expected
        .drain(..)
        .map(|k| match k {
            "type" => "plugin".to_string(),
            "type_version" => "plugin_version".to_string(),
            "source" => "origin".to_string(),
            "supersedes" => "revises".to_string(),
            other => other.to_string(),
        })
        .collect();
    for key in &renamed {
        assert!(review.get(key).is_some(), "missing {key} in {review}");
    }
    assert!(review["id"].as_str().unwrap().starts_with("r_"));
    assert_eq!(review["plugin"], "list");
    assert_eq!(review["plugin_version"], 1);
    assert_eq!(review["status"], "pending");
    assert_eq!(
        review["origin"],
        json!({"repo": "acme", "workflow": "review", "ref": "42", "run_id": "r1", "url": "https://x/42"})
    );
    assert_eq!(review["requested_by"], "agent");
    assert_eq!(review["summary"]["subtitle"], "2 items");
    assert_eq!(review["payload"], list_payload());
    assert_eq!(review["decision"], Value::Null);
    assert!(review["created_at"].as_str().unwrap().ends_with('Z'));
}

#[tokio::test]
async fn payload_violations_match_the_reference() {
    let app = app();
    for (name, payload) in [
        ("create-payload-invalid", json!({"intro": 1})),
        (
            "create-additional-property",
            json!({"groups": [], "extra": 1, "intro": "x"}),
        ),
        (
            "create-nested-invalid",
            json!({"groups": [{"title": "g", "items": [{"id": "x", "title": "t", "bogus": true}]}]}),
        ),
    ] {
        let body = json!({"plugin": "list", "title": "bad", "payload": payload});
        let (status, response) = call(&app, "POST", "/api/v1/reviews", Some(body)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{name}");
        assert_eq!(response["error"], "invalid");
        assert_eq!(violations(&response), fixture_violations(name), "{name}");
        assert_eq!(response["message"], fixture(name).1["message"], "{name}");
    }
}

#[tokio::test]
async fn envelope_violations_match_the_reference() {
    let app = app();
    let body = json!({"plugin": "list", "payload": {"groups": []}, "expires_at": "soon", "revises": "r_nope", "origin": "x"});
    let (status, response) = call(&app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        violations(&response),
        fixture_violations("create-envelope-invalid")
    );

    let (status, response) = call(
        &app,
        "POST",
        "/api/v1/reviews",
        Some(json!({"plugin": "nope", "title": "t"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        violations(&response),
        vec![("/plugin".to_string(), "unknown plugin nope".to_string())]
    );

    let (status, response) = call(&app, "POST", "/api/v1/reviews", Some(json!("nope"))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(violations(&response)[0].1, "must be a JSON object");
}

#[tokio::test]
async fn deciding_validates_records_and_then_refuses() {
    let app = app();
    let id = submit(&app, submission()).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let path = format!("/api/v1/reviews/{id}/decision");

    let (status, response) = call(
        &app,
        "POST",
        &path,
        Some(json!({"data": {"decisions": [{"id": 1, "action": "maybe"}]}})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(violations(&response), fixture_violations("decide-invalid"));

    let (status, response) = call(&app, "POST", &path, Some(json!({}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        violations(&response),
        fixture_violations("decide-missing-data")
    );

    let data = json!({"decisions": [{"id": 1, "action": "accept"}, {"id": 2, "action": "reject", "note": "no"}], "undecided": []});
    let (status, review) = call(
        &app,
        "POST",
        &path,
        Some(json!({"data": data, "agent_note": "  ship it  ", "decided_by": "spoof"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{review}");
    assert_eq!(review["status"], "decided");
    assert_eq!(review["decision"]["decided_by"], "tester");
    assert_eq!(review["decision"]["data"], data);
    assert_eq!(review["agent_note"], "ship it");

    let (status, response) = call(
        &app,
        "POST",
        &path,
        Some(json!({"data": {"decisions": [], "undecided": []}})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(response["error"], "not_pending");

    let (status, review) = call(&app, "GET", &format!("/api/v1/reviews/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(review["status"], "decided");
    assert_eq!(review["payload"], list_payload());

    let (status, waited) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews/{id}/wait?timeout=1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(waited["status"], "decided");

    let (status, events) = call(&app, "GET", &format!("/api/v1/reviews/{id}/events"), None).await;
    assert_eq!(status, StatusCode::OK);
    let kinds: Vec<&str> = events
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["created", "decided"]);
}

#[tokio::test]
async fn wait_times_out_with_204_and_wakes_on_a_decision() {
    let app = app();
    let id = submit(&app, submission()).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, body) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews/{id}/wait?timeout=1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(body, Value::Null);

    let reviews = app.state.reviews.clone();
    let decide_id = id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        reviews
            .decide(
                &decide_id,
                &json!({"decisions": [], "undecided": [1, 2]}),
                None,
            )
            .unwrap();
    });
    let started = std::time::Instant::now();
    let (status, review) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews/{id}/wait?timeout=10"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(review["status"], "decided");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn withdrawing_keeps_the_reason_and_then_refuses() {
    let app = app();
    let id = submit(&app, submission()).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let path = format!("/api/v1/reviews/{id}/withdraw");
    let (status, review) = call(
        &app,
        "POST",
        &path,
        Some(json!({"reason": "superseded by a rewrite"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{review}");
    assert_eq!(review["status"], "withdrawn");
    assert_eq!(review["withdrawn_reason"], "superseded by a rewrite");
    assert!(review["withdrawn_at"].is_string());
    let (status, _) = call(&app, "POST", &path, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/reviews/{id}/decision"),
        Some(json!({"data": {"decisions": [], "undecided": []}})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn unknown_reviews_are_404() {
    let app = app();
    let (status, body) = call(&app, "GET", "/api/v1/reviews/r_nope", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not_found");
    assert_eq!(body["message"], "review r_nope not found");
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/reviews/r_nope/decision",
        Some(json!({"data": {}})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&app, "GET", "/api/v1/reviews/r_nope/rounds", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn listing_filters_hides_revised_rounds_and_pages() {
    let app = app();
    let first = submit(&app, submission()).await;
    let first_id = first["id"].as_str().unwrap().to_string();
    let mut other = submission();
    other["origin"]["repo"] = json!("other");
    other["title"] = json!("other repo");
    let other_id = submit(&app, other).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut round2 = submission();
    round2["title"] = json!("round 2");
    round2["revises"] = json!(first_id);
    let round2_id = submit(&app, round2).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let ids = |body: &Value| -> Vec<String> {
        body.as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect()
    };
    let (_, all) = call(&app, "GET", "/api/v1/reviews", None).await;
    assert_eq!(ids(&all), vec![round2_id.clone(), other_id.clone()]);
    assert!(all[0].get("payload").is_none());

    let (_, with_revised) = call(&app, "GET", "/api/v1/reviews?include_revised=true", None).await;
    assert_eq!(
        ids(&with_revised),
        vec![round2_id.clone(), other_id.clone(), first_id.clone()]
    );

    let (_, acme) = call(
        &app,
        "GET",
        "/api/v1/reviews?repo=acme&status=pending",
        None,
    )
    .await;
    assert_eq!(ids(&acme), vec![round2_id.clone()]);

    let (_, searched) = call(&app, "GET", "/api/v1/reviews?q=other%20repo", None).await;
    assert_eq!(ids(&searched), vec![other_id.clone()]);

    let (_, paged) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews?limit=1&cursor={round2_id}"),
        None,
    )
    .await;
    assert_eq!(ids(&paged), vec![other_id.clone()]);

    let (status, response) = call(&app, "GET", "/api/v1/reviews?status=bogus", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(violations(&response), fixture_violations("list-bad-status"));

    let (status, rounds) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews/{round2_id}/rounds"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&rounds), vec![first_id.clone(), round2_id.clone()]);
    let (_, rounds_from_first) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews/{first_id}/rounds"),
        None,
    )
    .await;
    assert_eq!(ids(&rounds_from_first), vec![first_id, round2_id]);
}

#[tokio::test]
async fn expired_reviews_read_as_expired_and_are_swept_once() {
    let app = app();
    let mut body = submission();
    body["expires_at"] = json!("2020-01-01T00:00:00Z");
    let review = submit(&app, body).await;
    assert_eq!(review["status"], "expired");
    let id = review["id"].as_str().unwrap();
    let swept = app.state.reviews.sweep_expired().unwrap();
    assert_eq!(swept.len(), 1);
    assert!(app.state.reviews.sweep_expired().unwrap().is_empty());
    let (_, events) = call(&app, "GET", &format!("/api/v1/reviews/{id}/events"), None).await;
    let kinds: Vec<&str> = events
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["created", "expired"]);
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/reviews/{id}/decision"),
        Some(json!({"data": {"decisions": [], "undecided": []}})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn plugins_are_listed_added_and_reloaded() {
    let app = app();
    let (status, body) = call(&app, "GET", "/api/v1/plugins", None).await;
    assert_eq!(status, StatusCode::OK);
    let names = |body: &Value| -> Vec<String> {
        body["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(names(&body), vec!["list"]);
    assert_eq!(body["plugins"][0]["usable"], true);
    assert_eq!(body["dirs"].as_array().unwrap().len(), 2);

    let samples: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/plugins/dirs",
        Some(json!({"dir": samples.display().to_string()})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["count"], 5);
    let (_, body) = call(&app, "GET", "/api/v1/plugins", None).await;
    assert_eq!(
        names(&body),
        vec!["artifact", "email", "hello", "list", "review"]
    );
    // the artifact plugin is built from sources; unbuilt, it is listed as
    // broken with the reason, and everything else is usable
    for p in body["plugins"].as_array().unwrap() {
        if p["usable"] == true {
            continue;
        }
        assert_eq!(p["name"], "artifact", "{p}");
        assert!(
            p["error"]
                .as_str()
                .unwrap()
                .contains("entry index.html not found"),
            "{p}"
        );
    }
    assert_eq!(app.state.db.plugin_dirs().unwrap().len(), 1);

    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/plugins/dirs",
        Some(json!({"dir": "/nope/nowhere"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(violations(&body)[0].0, "/dir");

    let (status, body) = call(&app, "POST", "/api/v1/plugins/reload", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 5);

    let (status, _) = call(&app, "GET", "/api/v1/plugins/nope/versions", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    submit(&app, submission()).await;
    let (status, body) = call(&app, "GET", "/api/v1/plugins/list/versions", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["versions"], json!([1]));
    assert_eq!(body["current"], 1);
}

#[tokio::test]
async fn bundles_are_served_from_snapshots_with_the_sandbox_csp() {
    let app = app();
    let request = |path: &str| {
        Request::get(path)
            .header("host", "127.0.0.1:4747")
            .body(Body::empty())
            .unwrap()
    };
    // a snapshot is taken on first use, whether that is a submission or a
    // request for the bundle; a version that never existed has none
    let response = app
        .router
        .clone()
        .oneshot(request("/plugins/list/7/index.html"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = app
        .router
        .clone()
        .oneshot(request("/plugins/list/1/index.html"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "served, and snapshotted");
    assert!(
        app.state
            .config
            .snapshots_dir()
            .join("list")
            .join("1")
            .join("manifest.json")
            .is_file()
    );

    submit(&app, submission()).await;
    let response = app
        .router
        .clone()
        .oneshot(request("/plugins/list/1/index.html"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let csp = response.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .to_string();
    assert!(csp.starts_with("default-src 'none'; script-src 'unsafe-inline' http://127.0.0.1:4747/plugins/list/1/ http://127.0.0.1:4747/sdk/"), "{csp}");
    assert!(
        csp.ends_with(
            "connect-src 'none'; form-action 'none'; base-uri 'none'; frame-ancestors 'self' tauri://localhost http://tauri.localhost http://localhost:5173"
        ),
        "{csp}"
    );
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );

    for bad in [
        "/plugins/list/1/../manifest.json",
        "/plugins/list/2/index.html",
        "/plugins/nope/1/index.html",
        "/plugins/list/x/index.html",
    ] {
        let response = app.router.clone().oneshot(request(bad)).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{bad}");
    }
}

#[tokio::test]
async fn the_sdk_is_served_only_when_configured() {
    let app = app();
    let (status, _) = call(&app, "GET", "/sdk/v1/wicket-plugin.js", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("wicket-plugin.js"), "export const ok = 1;").unwrap();
    let mut config = Config::new(app._dir.path(), 0);
    config.sdk_dir = Some(dir.path().to_path_buf());
    let state = AppState::open(config).unwrap();
    let router = router(state);
    let response = router
        .oneshot(
            Request::get("/sdk/v1/wicket-plugin.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .contains("javascript")
    );
}

#[tokio::test]
async fn info_and_viewed() {
    let app = app();
    let (status, info) = call(&app, "GET", "/api/v1/info", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(info["version"], wicket_core::VERSION);
    assert_eq!(info["user"], "tester");
    let id = submit(&app, submission()).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, _) = call(&app, "POST", &format!("/api/v1/reviews/{id}/viewed"), None).await;
    assert_eq!(status, StatusCode::OK);
    let (_, events) = call(&app, "GET", &format!("/api/v1/reviews/{id}/events"), None).await;
    assert_eq!(events[1]["kind"], "viewed");
    assert_eq!(events[1]["actor"], "tester");
}
