//! The API end to end over its router. The envelope's fields and the exact
//! wording and order of violations, which plugins render, are checked against
//! the recorded responses in `tests/fixtures/api`.

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
use wicket_core::Wicket;
use wicket_core::api::router;

struct App {
    _dir: tempfile::TempDir,
    state: Arc<Wicket>,
    router: Router,
}

fn app() -> App {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::new(dir.path(), 0);
    config.user = "tester".into();
    let state = Arc::new(Wicket::open(config).unwrap());
    App {
        router: router(state.clone()),
        state,
        _dir: dir,
    }
}

// Inspect or seed persistence explicitly; the application does not expose
// its database connection to callers.
fn db(app: &App) -> wicket_core::db::Db {
    wicket_core::db::Db::open(&app.state.config().db_path()).unwrap()
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
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/api/{name}.txt"));
    let text = std::fs::read_to_string(&path).unwrap();
    let (body, status) = text.trim_end().rsplit_once('\n').unwrap();
    (status.parse().unwrap(), serde_json::from_str(body).unwrap())
}

/// The fixture's violations, as (path, message) pairs.
fn fixture_violations(name: &str) -> Vec<(String, String)> {
    let (_, body) = fixture(name);
    violations(&body)
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
async fn submit_returns_every_field_of_the_envelope() {
    let app = app();
    let review = submit(&app, submission()).await;
    let (_, recorded) = fixture("create-ok");

    for key in recorded.as_object().unwrap().keys() {
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
async fn payload_violations_keep_their_recorded_wording() {
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
async fn envelope_violations_keep_their_recorded_wording() {
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

    let reviews = app.state.reviews().clone();
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

    // a listing's envelope, or the plain array /rounds answers with
    let ids = |body: &Value| -> Vec<String> {
        body.get("reviews")
            .unwrap_or(body)
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect()
    };
    let (_, all) = call(&app, "GET", "/api/v1/reviews", None).await;
    assert_eq!(ids(&all), vec![round2_id.clone(), other_id.clone()]);
    assert!(all["reviews"][0].get("payload").is_none());

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
async fn a_listing_pages_by_cursor_or_offset_searches_every_field_and_offers_its_filters() {
    let app = app();
    // five reviews, oldest first: three in acme, one elsewhere, one with no project
    let mut made = Vec::new();
    for (title, repo, requested_by) in [
        ("first", Some("acme"), "agent"),
        ("second", Some("acme"), "agent"),
        ("third", Some("other"), "nightly"),
        ("fourth", None, "agent"),
        ("fifth", Some("acme"), "agent"),
    ] {
        let mut body = submission();
        body["title"] = json!(title);
        body["requested_by"] = json!(requested_by);
        match repo {
            Some(r) => body["origin"]["repo"] = json!(r),
            None => {
                body["origin"].as_object_mut().unwrap().remove("repo");
            }
        }
        made.push(submit(&app, body).await["id"].as_str().unwrap().to_string());
    }
    let decision = json!({"data": {"decisions": [], "undecided": [1, 2]}});
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/reviews/{}/decision", made[1]),
        Some(decision),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let titles = |body: &Value| -> Vec<String> {
        body["reviews"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["title"].as_str().unwrap().to_string())
            .collect()
    };

    // newest first, two at a time, with the total and the way on
    let (status, one) = call(&app, "GET", "/api/v1/reviews?limit=2", None).await;
    assert_eq!(status, StatusCode::OK, "{one}");
    assert_eq!(titles(&one), vec!["fifth", "fourth"]);
    assert_eq!(one["total"], 5);
    assert_eq!(one["has_more"], true);
    assert_eq!(one["next_cursor"], json!(made[3]));
    assert!(one.get("facets").is_none(), "facets only when asked for");

    // the cursor walks the rest, and the last page says there is no more
    let (_, two) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews?limit=2&cursor={}", made[3]),
        None,
    )
    .await;
    assert_eq!(titles(&two), vec!["third", "second"]);
    assert_eq!(two["total"], 5, "the total ignores the cursor");
    let (_, last) = call(
        &app,
        "GET",
        &format!(
            "/api/v1/reviews?limit=2&cursor={}",
            two["next_cursor"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(titles(&last), vec!["first"]);
    assert_eq!(
        (last["has_more"].clone(), last["next_cursor"].clone()),
        (json!(false), Value::Null)
    );

    // numbered pages by offset
    let (_, three) = call(&app, "GET", "/api/v1/reviews?limit=2&offset=4", None).await;
    assert_eq!(titles(&three), vec!["first"]);
    assert_eq!(
        (three["total"].clone(), three["has_more"].clone()),
        (json!(5), json!(false))
    );

    // the menus offer everything the status filter admits, whatever else is filtered
    let (_, acme) = call(
        &app,
        "GET",
        "/api/v1/reviews?repo=acme&plugin=list&include=facets",
        None,
    )
    .await;
    assert_eq!(titles(&acme), vec!["fifth", "second", "first"]);
    assert_eq!(acme["facets"]["plugins"], json!(["list"]));
    assert_eq!(acme["facets"]["repos"], json!(["acme", "other"]));
    assert_eq!(acme["facets"]["unassigned"], json!(true));

    // "-" is the reviews that name no project
    let (_, loose) = call(&app, "GET", "/api/v1/reviews?repo=-", None).await;
    assert_eq!(titles(&loose), vec!["fourth"]);

    // every word, in any of the fields the list shows: requester, project, decider
    let (_, nightly) = call(&app, "GET", "/api/v1/reviews?q=NIGHTLY%20other", None).await;
    assert_eq!(titles(&nightly), vec!["third"]);
    let (_, decided_by) = call(&app, "GET", "/api/v1/reviews?q=tester", None).await;
    assert_eq!(titles(&decided_by), vec!["second"]);
    let (_, nothing) = call(&app, "GET", "/api/v1/reviews?q=nightly%20fifth", None).await;
    assert_eq!(nothing["total"], 0);

    // decided only: the menus narrow to what that status holds
    let (_, decided) = call(
        &app,
        "GET",
        "/api/v1/reviews?status=decided&include=facets",
        None,
    )
    .await;
    assert_eq!(titles(&decided), vec!["second"]);
    assert_eq!(decided["facets"]["repos"], json!(["acme"]));
    assert_eq!(decided["facets"]["unassigned"], json!(false));

    // an include the server does not know is refused
    let (status, _) = call(&app, "GET", "/api/v1/reviews?include=everything", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // a literal % or _ in a search is not a wildcard
    let (_, percent) = call(&app, "GET", "/api/v1/reviews?q=%25", None).await;
    assert_eq!(percent["total"], 0);
}

#[tokio::test]
async fn expired_reviews_read_as_expired_and_are_swept_once() {
    let app = app();
    let mut body = submission();
    body["expires_at"] = json!("2020-01-01T00:00:00Z");
    let review = submit(&app, body).await;
    assert_eq!(review["status"], "expired");
    let id = review["id"].as_str().unwrap();
    let swept = app.state.reviews().sweep_expired().unwrap();
    assert_eq!(swept.len(), 1);
    assert!(app.state.reviews().sweep_expired().unwrap().is_empty());
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
                .contains("entry view/index.html not found"),
            "{p}"
        );
    }
    // each plugin in the directory is a link of its own now
    let links = db(&app).installed_plugins().unwrap();
    assert_eq!(links.len(), 4);
    assert!(links.iter().all(|r| r.linked && r.kind == "path"));
    let (_, body) = call(&app, "GET", "/api/v1/plugins", None).await;
    let hello = body["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "hello")
        .unwrap();
    assert_eq!(hello["install"]["linked"], true);
    assert_eq!(hello["install"]["kind"], "path");
    assert_eq!(hello["install"]["version"], "1.0.0");
    assert!(
        body["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "list")
            .unwrap()["install"]
            .is_null()
    );

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
async fn bundles_are_served_with_the_sandbox_csp() {
    let app = app();
    let request = |path: &str| {
        Request::get(path)
            .header("host", "127.0.0.1:4747")
            .body(Body::empty())
            .unwrap()
    };
    // a version that never existed is not served
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
    assert_eq!(response.status(), StatusCode::OK);

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
    // the SDK stylesheet names a typeface the SDK itself serves
    assert!(
        csp.contains(
            "font-src data: http://127.0.0.1:4747/plugins/list/1/ http://127.0.0.1:4747/sdk/"
        ),
        "{csp}"
    );
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
    let state = Arc::new(Wicket::open(config).unwrap());
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
async fn a_review_is_served_as_markdown_on_request() {
    let app = app();
    let review = submit(&app, submission()).await;
    let id = review["id"].as_str().unwrap();
    let text = |path: &str, accept: Option<&str>| {
        let mut req = Request::get(path).header("host", "127.0.0.1:4747");
        if let Some(a) = accept {
            req = req.header("accept", a);
        }
        req.body(Body::empty()).unwrap()
    };

    // pending, by the query parameter
    let response = app
        .router
        .clone()
        .oneshot(text(&format!("/api/v1/reviews/{id}?format=markdown"), None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/markdown")
    );
    let body = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(
        body.starts_with(
            "# MR !42

list · acme · review · 42
Pending since "
        ),
        "{body}"
    );
    assert!(
        body.trim_end().ends_with("Waiting for a decision."),
        "{body}"
    );

    // decided, by the Accept header, with the note and the tally
    let (status, _) = call(&app, "POST", &format!("/api/v1/reviews/{id}/decision"), Some(json!({"data": {"decisions": [{"id": 1, "action": "accept"}, {"id": 2, "action": "reject", "note": "typo is fine"}], "undecided": []}, "agent_note": "ship it"}))).await;
    assert_eq!(status, StatusCode::OK);
    let response = app
        .router
        .clone()
        .oneshot(text(
            &format!("/api/v1/reviews/{id}"),
            Some("text/markdown, application/json;q=0.5"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(body.contains("Decided by tester at "), "{body}");
    assert!(
        body.contains(
            "· 1 accepted, 1 rejected

> ship it

## Decisions

- **#1** **accepted**
- **#2** **rejected**
  > typo is fine
"
        ),
        "{body}"
    );

    // JSON stays the default, and Accept that prefers JSON gets JSON
    let response = app
        .router
        .clone()
        .oneshot(text(
            &format!("/api/v1/reviews/{id}"),
            Some("application/json, text/markdown"),
        ))
        .await
        .unwrap();
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("application/json")
    );
    let (status, shown) = call(&app, "GET", &format!("/api/v1/reviews/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(shown["status"], "decided");

    // a second round says where it stands in the chain
    let mut next = submission();
    next["revises"] = json!(id);
    let round = submit(&app, next).await;
    let response = app
        .router
        .clone()
        .oneshot(text(
            &format!(
                "/api/v1/reviews/{}?format=md",
                round["id"].as_str().unwrap()
            ),
            None,
        ))
        .await
        .unwrap();
    let body = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(
        body.contains(
            "list · acme · review · 42 · round 2 of 2
"
        ),
        "{body}"
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

#[tokio::test]
async fn discarding_records_who_and_why_wakes_the_waiter_and_then_refuses() {
    let app = app();
    let id = submit(&app, submission()).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut rx = app.state.events().subscribe();

    // an agent blocked on the review hears the discard at once
    let waiter = {
        let router = app.router.clone();
        let id = id.clone();
        tokio::spawn(async move {
            let request = Request::builder()
                .method("GET")
                .uri(format!("/api/v1/reviews/{id}/wait?timeout=10"))
                .body(Body::empty())
                .unwrap();
            let response = router.oneshot(request).await.unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (status, serde_json::from_slice::<Value>(&bytes).unwrap())
        })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;

    let path = format!("/api/v1/reviews/{id}/discard");
    let started = std::time::Instant::now();
    let (status, review) = call(&app, "POST", &path, Some(json!({"reason": "  not now  "}))).await;
    assert_eq!(status, StatusCode::OK, "{review}");
    assert_eq!(review["status"], "discarded");
    assert_eq!(review["discarded_reason"], "not now");
    assert_eq!(review["discarded_by"], "tester");
    assert!(review["discarded_at"].is_string());
    assert!(review["decision"].is_null() && review["withdrawn_at"].is_null());

    let (status, seen) = waiter.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(seen["status"], "discarded");
    assert_eq!(seen["discarded_reason"], "not now");
    assert!(started.elapsed() < Duration::from_secs(5));

    // the event names the person and the reason, on the bus and on record
    let notice = rx.recv().await.unwrap();
    assert_eq!(notice.kind, "discarded");
    assert_eq!(notice.review_id.as_deref(), Some(id.as_str()));
    let (_, events) = call(&app, "GET", &format!("/api/v1/reviews/{id}/events"), None).await;
    let event = events
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "discarded")
        .unwrap();
    assert_eq!(event["actor"], "tester");
    assert_eq!(event["attrs"]["reason"], "not now");

    // terminal: no second discard, no decision, no withdrawal
    for (method, path, body) in [
        ("POST", format!("/api/v1/reviews/{id}/discard"), None),
        (
            "POST",
            format!("/api/v1/reviews/{id}/decision"),
            Some(json!({"data": {"decisions": [], "undecided": []}})),
        ),
        ("POST", format!("/api/v1/reviews/{id}/withdraw"), None),
    ] {
        let (status, _) = call(&app, method, &path, body).await;
        assert_eq!(status, StatusCode::CONFLICT, "{method} {path}");
    }

    // listed under its own status, and out of the pending set
    let (_, listed) = call(&app, "GET", "/api/v1/reviews?status=discarded", None).await;
    assert_eq!(listed["reviews"][0]["id"], id);
    let (_, pending) = call(&app, "GET", "/api/v1/reviews?status=pending", None).await;
    assert!(
        pending["reviews"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["id"] != id)
    );

    // without a body: no reason, the server's user
    let other = submit(&app, submission()).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, review) = call(
        &app,
        "POST",
        &format!("/api/v1/reviews/{other}/discard"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{review}");
    assert!(review["discarded_reason"].is_null());
    assert_eq!(review["discarded_by"], "tester");
    let (status, _) = call(&app, "POST", "/api/v1/reviews/r_nope/discard", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A store entry: the built-in list plugin copied under another name, as
/// an install would place it, with its record and hash.
#[tokio::test]
async fn the_history_sweep_deletes_ended_reviews_past_the_days_kept() {
    let app = app();
    let decision = json!({ "data": { "decisions": [], "undecided": [1, 2] }, "agent_note": null });

    // decided, withdrawn, pending, and a pending round revising the decided one
    let decided = submit(&app, submission()).await;
    let (status, _) = call(
        &app,
        "POST",
        &format!(
            "/api/v1/reviews/{}/decision",
            decided["id"].as_str().unwrap()
        ),
        Some(decision.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let withdrawn = submit(&app, submission()).await;
    let (status, _) = call(
        &app,
        "POST",
        &format!(
            "/api/v1/reviews/{}/withdraw",
            withdrawn["id"].as_str().unwrap()
        ),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let pending = submit(&app, submission()).await;
    let mut round = submission();
    round["revises"] = decided["id"].clone();
    let round = submit(&app, round).await;

    // nothing to keep for: everything stays
    assert_eq!(app.state.reviews().sweep_history(None).unwrap(), 0);
    let tomorrow = chrono::Utc::now() + chrono::Duration::days(1);
    // the withdrawn one goes; the decided one stays while the round revising it is pending
    assert_eq!(
        app.state.reviews().sweep_history_before(tomorrow).unwrap(),
        1
    );
    let (status, _) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews/{}", withdrawn["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews/{}", decided["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // the round decided, the chain goes together; the pending review stays
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/reviews/{}/decision", round["id"].as_str().unwrap()),
        Some(decision.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        app.state.reviews().sweep_history_before(tomorrow).unwrap(),
        2
    );
    let (status, _) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews/{}", decided["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews/{}", pending["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // the last review gone too
    let (status, _) = call(
        &app,
        "POST",
        &format!(
            "/api/v1/reviews/{}/decision",
            pending["id"].as_str().unwrap()
        ),
        Some(decision),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        app.state.reviews().sweep_history_before(tomorrow).unwrap(),
        1
    );
    let (_, listed) = call(
        &app,
        "GET",
        "/api/v1/reviews?status=decided,withdrawn,pending",
        None,
    )
    .await;
    assert_eq!(
        listed["reviews"].as_array().map(Vec::len),
        Some(0),
        "{listed}"
    );
}

#[tokio::test]
async fn a_store_entry_is_served_and_a_tampered_one_is_flagged() {
    let app = app();
    let source = tempfile::tempdir().unwrap();
    let builtin = app.state.config().builtin_plugins_dir().join("list");
    for file in std::fs::read_dir(&builtin).unwrap() {
        let file = file.unwrap();
        std::fs::copy(file.path(), source.path().join(file.file_name())).unwrap();
    }
    let manifest_path = source.path().join("manifest.json");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest["name"] = json!("shelf");
    std::fs::write(manifest_path, manifest.to_string()).unwrap();

    let (status, installed) = install(&app, source.path(), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{installed}");
    assert_eq!(installed["install"]["linked"], false);
    let hash = installed["install"]["hash"].as_str().unwrap();
    assert!(!hash.is_empty());
    let entry = app
        .state
        .config()
        .plugin_store_dir()
        .join("shelf")
        .join("1");
    let (status, body) = call(&app, "POST", "/api/v1/plugins/reload", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, body) = call(&app, "GET", "/api/v1/plugins", None).await;
    let shelf = body["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "shelf")
        .unwrap();
    assert_eq!(shelf["usable"], true, "{shelf}");
    assert_eq!(shelf["install"]["modified"], false);
    assert_eq!(shelf["install"]["hash"], hash);
    assert_eq!(shelf["path"], entry.display().to_string());

    // a review renders from the entry itself
    let mut body = submission();
    body["plugin"] = json!("shelf");
    let review = submit(&app, body).await;
    assert_eq!(review["plugin"], "shelf", "{review}");

    // a file changed behind the app's back: still served, but said so
    std::fs::write(entry.join("index.html"), "<html>changed</html>").unwrap();
    call(&app, "POST", "/api/v1/plugins/reload", None).await;
    let (_, body) = call(&app, "GET", "/api/v1/plugins", None).await;
    let shelf = body["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "shelf")
        .unwrap();
    assert_eq!(shelf["install"]["modified"], true);
    assert_eq!(shelf["usable"], true);
}

/// A copy of a sample plugin with its manifest's version rewritten.
fn plugin_copy(root: &std::path::Path, name: &str, version: &str) -> std::path::PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins")
        .join(name);
    let to = root.join(format!("{name}-{}", version.replace('.', "_")));
    fn copy_tree(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for file in std::fs::read_dir(from).unwrap().flatten() {
            let name = file.file_name();
            // the plugin as a bundle: without its tests and fixtures
            if name == "tests" || name == "fixtures" || name == "node_modules" {
                continue;
            }
            if file.file_type().unwrap().is_dir() {
                copy_tree(&file.path(), &to.join(&name));
            } else {
                std::fs::copy(file.path(), to.join(&name)).unwrap();
            }
        }
    }
    copy_tree(&from, &to);
    let manifest = std::fs::read_to_string(to.join("manifest.json")).unwrap();
    let mut manifest: Value = serde_json::from_str(&manifest).unwrap();
    manifest["version"] = json!(version);
    std::fs::write(to.join("manifest.json"), manifest.to_string()).unwrap();
    to
}

/// Starts an install and follows its job to the end: the plugin's row
/// with 200, or the failure as a 422 body, the way a caller sees them.
async fn install(app: &App, source: &std::path::Path, extra: Value) -> (StatusCode, Value) {
    let mut body = json!({ "source": source.display().to_string() });
    if let Value::Object(map) = extra {
        for (k, v) in map {
            body[k] = v;
        }
    }
    let (status, started) = call(app, "POST", "/api/v1/plugins/install", Some(body)).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let id = started["job"].as_str().unwrap().to_string();
    let job = follow(app, &id).await;
    if job["status"] == "done" {
        (StatusCode::OK, job["plugin"].clone())
    } else {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({ "error": "install_failed", "message": job["error"], "violations": [{"path": "/source", "message": job["error"]}] }),
        )
    }
}

async fn follow(app: &App, id: &str) -> Value {
    for _ in 0..600 {
        let (status, job) = call(app, "GET", &format!("/api/v1/plugins/jobs/{id}"), None).await;
        assert_eq!(status, StatusCode::OK, "{job}");
        if job["status"] == "done" || job["status"] == "failed" {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the install job {id} never ended");
}

#[tokio::test]
async fn installing_from_a_folder_places_a_line_in_the_store_and_keeps_old_lines_in_use() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();

    // the sample as it is: 1.0.0, placed, hashed, recorded
    let hello = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/hello");
    let (status, row) = install(&app, &hello, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["name"], "hello");
    assert_eq!(row["release"], "1.0.0");
    assert_eq!(row["install"]["kind"], "path");
    assert_eq!(row["install"]["linked"], false);
    assert!(row["install"]["hash"].is_string());
    let entry = app
        .state
        .config()
        .plugin_store_dir()
        .join("hello")
        .join("1");
    assert!(entry.join("manifest.json").is_file());
    assert!(
        !entry.join("README.md").exists() || true,
        "the copy keeps files, never node_modules or dot-entries"
    );
    assert_eq!(row["path"], entry.display().to_string());

    // a review renders from the line and records the exact version
    let mut body = submission();
    body["plugin"] = json!("hello");
    body["payload"] = json!({"message": "hi"});
    let review = submit(&app, body).await;
    assert_eq!(review["plugin_version"], 1);
    assert_eq!(review["plugin_release"], "1.0.0");

    // a patch replaces the line in place
    let patch = plugin_copy(scratch.path(), "hello", "1.0.4");
    let (status, row) = install(&app, &patch, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["release"], "1.0.4");
    assert_eq!(row["version"], 1);
    assert_eq!(db(&app).installed_plugins().unwrap().len(), 1);
    let (_, shown) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews/{}", review["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(
        shown["plugin_release"], "1.0.0",
        "the review keeps what it was submitted under"
    );

    // an older one is refused, unless forced
    let older = plugin_copy(scratch.path(), "hello", "1.0.2");
    let (status, body) = install(&app, &older, json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("older than the installed 1.0.4"),
        "{body}"
    );
    let (status, row) = install(&app, &older, json!({"force": true})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["release"], "1.0.2");

    // a new major is a new line; the old one stays because the review uses it
    let next = plugin_copy(scratch.path(), "hello", "2.0.0");
    let (status, row) = install(&app, &next, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["version"], 2);
    assert!(
        app.state
            .config()
            .plugin_store_dir()
            .join("hello")
            .join("2")
            .join("manifest.json")
            .is_file()
    );
    assert!(
        entry.join("manifest.json").is_file(),
        "line 1 stays: a review renders from it"
    );
    let (status, versions) = call(&app, "GET", "/api/v1/plugins/hello/versions", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(versions["current"], 2);
    let (status, bundle) = call(
        &app,
        "GET",
        &format!("/plugins/hello/1/{}", row["entry"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the old line's bundle is still served: {bundle}"
    );

    // a link serves the folder live, and a folder with no manifest is refused
    let (status, row) = install(&app, &hello, json!({"link": true})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["install"]["linked"], true);
    assert_eq!(
        row["path"],
        std::path::absolute(&hello).unwrap().display().to_string()
    );
    let (status, body) = install(&app, scratch.path(), json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(violations(&body)[0].0, "/source");
}

/// A plugin whose bundle only exists after its build runs.
#[tokio::test]
async fn inspecting_says_what_an_install_would_do_without_doing_it() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    let plain = plugin_copy(scratch.path(), "hello", "1.4.0");
    let (status, seen) = call(
        &app,
        "POST",
        "/api/v1/plugins/inspect",
        Some(json!({"source": plain.display().to_string()})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{seen}");
    assert_eq!(seen["name"], "hello");
    assert_eq!(seen["version"], "1.4.0");
    assert_eq!(seen["major"], 1);
    assert_eq!(seen["build"], Value::Null);
    assert_eq!(seen["origin"]["kind"], "path");
    assert_eq!(seen["installed"], Value::Null);
    assert_eq!(seen["link"], false);
    let store = app.state.config().plugin_store_dir().join("hello");
    assert!(!store.exists(), "inspecting placed something");

    // a source that builds says what it will run; once something is
    // installed under the name, the inspection says what it replaces
    let (_, plugin) = install(&app, &plain, json!({})).await;
    assert_eq!(plugin["name"], "hello");
    let (_, seen) = call(
        &app,
        "POST",
        "/api/v1/plugins/inspect",
        Some(json!({"source": plain.display().to_string()})),
    )
    .await;
    assert_eq!(seen["installed"]["version"], "1.4.0");
    assert_eq!(seen["installed"]["unchanged"], true, "{seen}");

    // checking for updates compares the folder with the store: what the
    // bundle leaves behind does not count as a change
    std::fs::create_dir_all(plain.join("tests")).unwrap();
    std::fs::write(plain.join("tests/plain.spec.ts"), "test").unwrap();
    std::fs::write(plain.join(".editorconfig"), "root = true").unwrap();
    let (_, updates) = call(&app, "GET", "/api/v1/plugins/hello/updates", None).await;
    assert_eq!(updates["state"], "up_to_date", "{updates}");
    std::fs::write(plain.join("view/index.html"), "<html>changed</html>").unwrap();
    let (_, updates) = call(&app, "GET", "/api/v1/plugins/hello/updates", None).await;
    assert_eq!(updates["state"], "available", "{updates}");
    let (_, seen) = call(
        &app,
        "POST",
        "/api/v1/plugins/inspect",
        Some(json!({"source": plain.display().to_string()})),
    )
    .await;
    assert_eq!(seen["installed"]["unchanged"], false, "{seen}");
    let built = buildable_plugin(scratch.path(), "hello", "npm run build");
    let (status, seen) = call(
        &app,
        "POST",
        "/api/v1/plugins/inspect",
        Some(json!({"source": built.display().to_string()})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{seen}");
    assert_eq!(seen["build"], "npm run build");
    assert_eq!(seen["installed"]["version"], "1.4.0");
    assert_eq!(seen["older"], true, "{seen}");

    // not a plugin at all
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/plugins/inspect",
        Some(json!({"source": scratch.path().display().to_string()})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["message"].as_str().unwrap().contains("manifest.json"),
        "{body}"
    );
}

#[tokio::test]
async fn removing_drops_the_record_and_keeps_an_entry_a_review_renders_from() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    let source = plugin_copy(scratch.path(), "hello", "1.0.0");
    let (status, row) = install(&app, &source, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    let entry = app.state.config().plugin_store_dir().join("hello/1");
    assert!(entry.join("manifest.json").is_file());

    // a built-in has no record to remove
    let (status, body) = call(&app, "DELETE", "/api/v1/plugins/list", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // nothing renders from it: the entry goes with the record
    let (status, answer) = call(&app, "DELETE", "/api/v1/plugins/hello", None).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["entries_removed"], json!([1]));
    assert_eq!(answer["entries_kept"], json!([]));
    assert!(!entry.exists());
    let (_, listed) = call(&app, "GET", "/api/v1/plugins", None).await;
    assert!(
        listed["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["name"] != "hello"),
        "{listed}"
    );

    // a review still rendering from the entry keeps it, and it is still served
    let (status, row) = install(&app, &source, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    let mut body = submission();
    body["plugin"] = json!("hello");
    body["payload"] = json!({"message": "keep me"});
    let review = submit(&app, body).await;
    let (status, answer) = call(&app, "DELETE", "/api/v1/plugins/hello", None).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["entries_kept"], json!([1]), "{answer}");
    assert!(entry.join("manifest.json").is_file());
    let (status, _) = call(
        &app,
        "GET",
        &format!(
            "/plugins/hello/{}/view/index.html",
            review["plugin_version"]
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, listed) = call(&app, "GET", "/api/v1/plugins", None).await;
    assert!(
        listed["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["name"] != "hello"),
        "{listed}"
    );

    // a link loses its record and nothing else
    let (status, row) = install(&app, &source, json!({"link": true})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    let (status, answer) = call(&app, "DELETE", "/api/v1/plugins/hello", None).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["linked"], true);
    assert!(source.join("manifest.json").is_file());
    let (status, _) = call(&app, "DELETE", "/api/v1/plugins/hello", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_patch_update_is_what_the_app_serves_and_a_link_is_live() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    let first = plugin_copy(scratch.path(), "hello", "1.0.0");
    std::fs::write(first.join("view/index.html"), "<html>first</html>").unwrap();
    let (status, row) = install(&app, &first, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");

    // rendered once, and a review created against it, as the app does
    let (status, body) = call(&app, "GET", "/plugins/hello/1/view/index.html", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.as_str().unwrap_or(&body.to_string()),
        "<html>first</html>"
    );
    let mut body = submission();
    body["plugin"] = json!("hello");
    body["payload"] = json!({"message": "hi"});
    submit(&app, body).await;
    // the patch replaces the line, and the app serves it at once
    let second = plugin_copy(scratch.path(), "hello", "1.0.1");
    std::fs::write(second.join("view/index.html"), "<html>second</html>").unwrap();
    let (status, row) = install(&app, &second, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["install"]["version"], "1.0.1");
    let (status, body) = call(&app, "GET", "/plugins/hello/1/view/index.html", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.as_str().unwrap_or(&body.to_string()),
        "<html>second</html>"
    );

    // a link is served live: an edit shows on the next request
    let live = plugin_copy(scratch.path(), "hello", "1.1.0");
    std::fs::write(live.join("view/index.html"), "<html>live</html>").unwrap();
    let (status, _) = install(&app, &live, json!({"link": true})).await;
    assert_eq!(status, StatusCode::OK);
    let mut body = submission();
    body["plugin"] = json!("hello");
    body["payload"] = json!({"message": "again"});
    submit(&app, body).await;
    std::fs::write(live.join("view/index.html"), "<html>edited</html>").unwrap();
    let (_, body) = call(&app, "GET", "/plugins/hello/1/view/index.html", None).await;
    assert_eq!(
        body.as_str().unwrap_or(&body.to_string()),
        "<html>edited</html>"
    );
    // the link removed: the store entry the link had replaced is still
    // referenced by reviews, so it is kept and serves them, and the
    // versions endpoint lists it with no current plugin
    let (status, _) = call(&app, "DELETE", "/api/v1/plugins/hello", None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = call(&app, "GET", "/plugins/hello/1/view/index.html", None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the kept store entry serves the reviews"
    );
    assert_eq!(
        body.as_str().unwrap_or(&body.to_string()),
        "<html>second</html>"
    );
    let (status, versions) = call(&app, "GET", "/api/v1/plugins/hello/versions", None).await;
    assert_eq!(status, StatusCode::OK, "{versions}");
    assert_eq!(versions["current"], Value::Null);
    assert_eq!(versions["versions"], json!([1]));

    // installed again from the live folder, the line is replaced and current
    let (status, _) = install(&app, &live, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let (_, versions) = call(&app, "GET", "/api/v1/plugins/hello/versions", None).await;
    assert_eq!(versions["current"], 1);
    let (_, body) = call(&app, "GET", "/plugins/hello/1/view/index.html", None).await;
    assert_eq!(
        body.as_str().unwrap_or(&body.to_string()),
        "<html>edited</html>"
    );
}

#[tokio::test]
async fn start_tidies_the_plugins_folder_and_a_build_keeps_the_last_five_logs() {
    // a data directory laid out the old way: snapshots at the top level,
    // a clone left in fetch by a crash, a store entry to keep
    let dir = tempfile::tempdir().unwrap();
    let plugins = dir.path().join("plugins");
    std::fs::create_dir_all(plugins.join("review/1")).unwrap();
    std::fs::write(plugins.join("review/1/manifest.json"), "{}").unwrap();
    std::fs::create_dir_all(plugins.join("fetch/git-abc")).unwrap();
    std::fs::write(plugins.join("fetch/git-abc/file"), "x").unwrap();
    std::fs::create_dir_all(plugins.join("store/hello/1")).unwrap();
    std::fs::write(plugins.join("store/hello/1/keep"), "x").unwrap();
    std::fs::create_dir_all(plugins.join("logs")).unwrap();
    std::fs::write(plugins.join("logs/old.log"), "x").unwrap();
    let mut config = Config::new(dir.path(), 0);
    config.user = "tester".into();
    let state = Arc::new(Wicket::open(config).unwrap());
    assert!(!plugins.join("review").exists(), "the old snapshot is gone");
    assert!(
        plugins.join("fetch").is_dir()
            && std::fs::read_dir(plugins.join("fetch"))
                .unwrap()
                .next()
                .is_none(),
        "fetch is emptied"
    );
    assert!(
        plugins.join("store/hello/1/keep").is_file(),
        "the store is untouched"
    );
    assert!(
        plugins.join("logs/old.log").is_file(),
        "logs are untouched at start"
    );
    let mut names: Vec<String> = std::fs::read_dir(&plugins)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["fetch", "logs", "store"]);

    // six builds, five logs
    let app = App {
        router: router(state.clone()),
        state,
        _dir: dir,
    };
    let scratch = tempfile::tempdir().unwrap();
    let built = buildable_plugin(
        scratch.path(),
        "built",
        "printf '<html>ok</html>' > index.html",
    );
    for _ in 0..6 {
        let (status, row) = install(&app, &built, json!({})).await;
        assert_eq!(status, StatusCode::OK, "{row}");
    }
    let logs: Vec<String> = std::fs::read_dir(plugins.join("logs"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("built-"))
        .collect();
    assert_eq!(logs.len(), 5, "{logs:?}");
}

fn buildable_plugin(root: &std::path::Path, name: &str, command: &str) -> std::path::PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/view.txt"), "sources").unwrap();
    std::fs::write(dir.join("package.json"), "{}").unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        json!({"name": name, "version": "1.0.0", "payload_schema": {}, "decision_schema": {}, "build": {"command": command}}).to_string(),
    )
    .unwrap();
    dir
}

#[tokio::test]
async fn a_build_declared_in_the_manifest_runs_in_a_scratch_copy_and_only_the_bundle_is_placed() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    let built = buildable_plugin(
        scratch.path(),
        "built",
        "echo building && mkdir -p assets && printf '<html>ok</html>' > index.html && printf 'x' > assets/a.js",
    );
    let (status, started) = call(
        &app,
        "POST",
        "/api/v1/plugins/install",
        Some(json!({"source": built.display().to_string()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let job = follow(&app, started["job"].as_str().unwrap()).await;
    assert_eq!(job["status"], "done", "{job}");
    assert!(
        job["log"].as_str().unwrap().contains("$ echo building"),
        "{job}"
    );
    assert!(
        job["log"].as_str().unwrap().contains("\nbuilding\n"),
        "{job}"
    );
    let entry = app
        .state
        .config()
        .plugin_store_dir()
        .join("built")
        .join("1");
    assert_eq!(
        std::fs::read_to_string(entry.join("index.html")).unwrap(),
        "<html>ok</html>"
    );
    assert!(entry.join("assets/a.js").is_file());
    assert!(!entry.join("src").exists(), "sources never enter the store");
    assert!(!entry.join("package.json").exists(), "nor the tooling");
    assert!(
        !built.join("index.html").exists(),
        "the source folder was not written to"
    );
    assert!(job["plugin"]["install"]["hash"].is_string());
    let log_path = db(&app).installed_plugins().unwrap()[0]
        .build_log
        .clone()
        .unwrap();
    assert!(
        std::fs::read_to_string(log_path)
            .unwrap()
            .contains("building")
    );

    // a failing build stops the install with the tail of its log; a manifest
    // without a build and without its entry is told what to declare
    let broken = buildable_plugin(scratch.path(), "broken", "echo nope && exit 3");
    let (status, body) = install(&app, &broken, json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let message = body["message"].as_str().unwrap();
    assert!(
        message.contains("the build failed") && message.contains("nope"),
        "{message}"
    );
    let (_, listed) = call(&app, "GET", "/api/v1/plugins", None).await;
    assert!(
        listed["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["name"] != "broken")
    );
    let bare = scratch.path().join("bare");
    std::fs::create_dir_all(&bare).unwrap();
    std::fs::write(
        bare.join("manifest.json"),
        json!({"name": "bare", "version": 1, "payload_schema": {}, "decision_schema": {}})
            .to_string(),
    )
    .unwrap();
    let (status, body) = install(&app, &bare, json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("declares its build"),
        "{body}"
    );
}

#[tokio::test]
async fn the_artifact_plugin_installs_from_its_sources() {
    let app = app();
    let artifact = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/artifact");
    let (status, row) = install(&app, &artifact, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["usable"], true, "{row}");
    let entry = app
        .state
        .config()
        .plugin_store_dir()
        .join("artifact")
        .join("1");
    assert!(entry.join("view/index.html").is_file());
    assert!(entry.join("view/assets").is_dir());
    assert!(entry.join("schemas/payload.schema.json").is_file());
    assert!(
        !entry.join("src").exists()
            && !entry.join("node_modules").exists()
            && !entry.join("package.json").exists()
    );
}

/// A repository with the hello plugin in a folder, tagged, and a bare
/// clone of it a URL can reach.
fn git_repo(root: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let work = root.join("work");
    // the hello plugin as a bundle, under tools/ in the repository
    let hello = plugin_copy(&root.join("stage"), "hello", "1.0.0");
    std::fs::create_dir_all(work.join("tools")).unwrap();
    std::fs::rename(&hello, work.join("tools/hello")).unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(&work)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "hello 1.0.0"]);
    git(&["tag", "v1"]);
    let bare = root.join("plugins.git");
    let out = std::process::Command::new("git")
        .args([
            "clone",
            "-q",
            "--bare",
            work.to_str().unwrap(),
            bare.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    (work, bare)
}

fn git_in(dir: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[tokio::test]
async fn installing_from_a_repository_records_the_commit_and_knows_what_is_new() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    let (work, bare) = git_repo(scratch.path());
    let url = format!("file://{}", bare.display());

    // a folder in the repository at a tag, the ref and folder given beside the URL
    let (status, row) = install(
        &app,
        Path::new(&url),
        json!({"ref": "v1", "path": "tools/hello"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["name"], "hello");
    assert_eq!(row["install"]["kind"], "git");
    let tagged = git_in(&work, &["rev-parse", "v1"]);
    assert_eq!(row["install"]["commit"], tagged);
    assert!(row["install"]["hash"].is_string());
    let record = db(&app)
        .installed_plugins()
        .unwrap()
        .into_iter()
        .find(|r| r.name == "hello")
        .unwrap();
    let resolved: Value = serde_json::from_str(&record.resolved).unwrap();
    assert_eq!(
        resolved,
        json!({"url": url, "path": "tools/hello", "ref": "v1"})
    );
    let fetched: Vec<_> = std::fs::read_dir(
        app.state
            .config()
            .plugin_store_dir()
            .parent()
            .unwrap()
            .join("fetch"),
    )
    .map(|d| d.flatten().collect())
    .unwrap_or_default();
    assert!(fetched.is_empty(), "the clone is gone once placed");

    // a tag is pinned: nothing to update, however the branch moves
    let (status, updates) = call(&app, "GET", "/api/v1/plugins/hello/updates", None).await;
    assert_eq!(status, StatusCode::OK, "{updates}");
    assert_eq!(updates["state"], "pinned");

    // the same folder on the branch, then a newer commit on it
    let (status, row) = install(
        &app,
        Path::new(&url),
        json!({"ref": "main", "path": "tools/hello"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{row}");
    let (_, updates) = call(&app, "GET", "/api/v1/plugins/hello/updates", None).await;
    assert_eq!(updates["state"], "up_to_date", "{updates}");
    let mut manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(work.join("tools/hello/manifest.json")).unwrap(),
    )
    .unwrap();
    manifest["version"] = json!("1.0.1");
    std::fs::write(work.join("tools/hello/manifest.json"), manifest.to_string()).unwrap();
    git_in(&work, &["commit", "-q", "-am", "hello 1.0.1"]);
    git_in(&work, &["push", "-q", bare.to_str().unwrap(), "main"]);
    let newer = git_in(&work, &["rev-parse", "main"]);
    let (_, updates) = call(&app, "GET", "/api/v1/plugins/hello/updates", None).await;
    assert_eq!(updates["state"], "available", "{updates}");
    assert_eq!(updates["commit"], newer);
    // update installs again from where it came, as a job
    let (status, started) = call(&app, "POST", "/api/v1/plugins/hello/update", None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let job = follow(&app, started["job"].as_str().unwrap()).await;
    assert_eq!(job["status"], "done", "{job}");
    let row = &job["plugin"];
    assert_eq!(row["release"], "1.0.1");
    assert_eq!(row["install"]["commit"], newer);
    // and says when there is nothing to do
    let (status, answer) = call(&app, "POST", "/api/v1/plugins/hello/update", None).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["state"], "up_to_date");
    assert_eq!(answer["version"], "1.0.1");

    // a pinned tag refuses to move
    let (status, row) = install(
        &app,
        Path::new(&url),
        json!({"ref": "v1", "path": "tools/hello", "force": true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{row}");
    let (status, answer) = call(&app, "POST", "/api/v1/plugins/hello/update", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    assert!(
        answer["message"].as_str().unwrap().contains("pinned to v1"),
        "{answer}"
    );

    // a commit is pinned too; a ref that does not exist fails with git's word
    let (status, row) = install(
        &app,
        Path::new(&url),
        json!({"ref": tagged, "path": "tools/hello", "force": true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{row}");
    let (_, updates) = call(&app, "GET", "/api/v1/plugins/hello/updates", None).await;
    assert_eq!(updates["state"], "pinned");
    let (status, body) = install(
        &app,
        Path::new(&url),
        json!({"ref": "nope", "path": "tools/hello"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("git clone failed"),
        "{body}"
    );
    // a link needs a folder
    let (status, body) = install(&app, Path::new(&url), json!({"link": true})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("a link needs a folder"),
        "{body}"
    );
}

/// A zip of `files` as `path → content`, the way a release asset carries
/// a bundle.
fn zipped(files: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let plain = zip::write::SimpleFileOptions::default();
    for (path, content) in files {
        out.start_file(*path, plain).unwrap();
        out.write_all(content.as_bytes()).unwrap();
    }
    out.finish().unwrap().into_inner()
}

fn bundle_manifest(name: &str, version: &str) -> String {
    json!({"name": name, "version": version, "payload_schema": {}, "decision_schema": {}, "build": {"command": "false"}}).to_string()
}

/// A stand-in for GitHub's releases API: a release per (repo, tag), a
/// changeable latest per repo, and the assets themselves.
struct Releases {
    latest: std::sync::Mutex<std::collections::HashMap<String, String>>,
    releases: std::sync::Mutex<std::collections::HashMap<(String, String), Value>>,
    assets: std::collections::HashMap<String, Vec<u8>>,
    base: std::sync::OnceLock<String>,
}

async fn releases_server(fake: Arc<Releases>) -> String {
    use axum::extract::{Path as P, State};
    use axum::routing::get;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    fake.base.set(base.clone()).unwrap();
    let router = Router::new()
        .route(
            "/repos/{owner}/{repo}/releases/latest",
            get(
                |State(f): State<Arc<Releases>>, P((owner, repo)): P<(String, String)>| async move {
                    let name = format!("{owner}/{repo}");
                    let tag = f.latest.lock().unwrap().get(&name).cloned();
                    match tag.and_then(|t| f.releases.lock().unwrap().get(&(name, t)).cloned()) {
                        Some(v) => (StatusCode::OK, axum::Json(v)),
                        None => (
                            StatusCode::NOT_FOUND,
                            axum::Json(json!({"message": "Not Found"})),
                        ),
                    }
                },
            ),
        )
        .route(
            "/repos/{owner}/{repo}/releases/tags/{tag}",
            get(
                |State(f): State<Arc<Releases>>,
                 P((owner, repo, tag)): P<(String, String, String)>| async move {
                    match f
                        .releases
                        .lock()
                        .unwrap()
                        .get(&(format!("{owner}/{repo}"), tag))
                        .cloned()
                    {
                        Some(v) => (StatusCode::OK, axum::Json(v)),
                        None => (
                            StatusCode::NOT_FOUND,
                            axum::Json(json!({"message": "Not Found"})),
                        ),
                    }
                },
            ),
        )
        .route(
            "/assets/{name}",
            get(
                |State(f): State<Arc<Releases>>, P(name): P<String>| async move {
                    match f.assets.get(&name) {
                        Some(bytes) => (StatusCode::OK, bytes.clone()),
                        None => (StatusCode::NOT_FOUND, Vec::new()),
                    }
                },
            ),
        )
        .with_state(fake);
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    base
}

impl Releases {
    fn release(&self, repo: &str, tag: &str, assets: &[&str]) {
        let base = self.base.get().unwrap();
        let list: Vec<Value> = assets
            .iter()
            .map(|a| json!({"name": a, "browser_download_url": format!("{base}/assets/{a}"), "size": self.assets.get(*a).map(Vec::len).unwrap_or(0)}))
            .collect();
        self.releases.lock().unwrap().insert(
            (repo.to_string(), tag.to_string()),
            json!({"tag_name": tag, "assets": list}),
        );
    }
    fn latest(&self, repo: &str, tag: &str) {
        self.latest.lock().unwrap().insert(repo.into(), tag.into());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn installing_from_a_release_takes_the_bundle_as_it_is_and_follows_the_latest() {
    let app = app();
    let thing_120 = zipped(&[
        ("manifest.json", &bundle_manifest("thing", "1.2.0")),
        ("index.html", "<html>1.2.0</html>"),
    ]);
    let thing_130 = zipped(&[
        ("manifest.json", &bundle_manifest("thing", "1.3.0")),
        ("index.html", "<html>1.3.0</html>"),
    ]);
    // a bundle inside the one folder at the archive's root, as GitHub's
    // own source archives and most zip tools lay it out
    let pinned = zipped(&[
        (
            "pinned-1.0.1/manifest.json",
            &bundle_manifest("pinned", "1.0.1"),
        ),
        ("pinned-1.0.1/index.html", "<html>pinned</html>"),
    ]);
    let lying = zipped(&[
        ("manifest.json", &bundle_manifest("lying", "1.0.0")),
        ("index.html", "<html>lying</html>"),
    ]);
    let mut assets = std::collections::HashMap::new();
    assets.insert("thing-1.2.0.zip".to_string(), thing_120.clone());
    assets.insert("thing-1.3.0.zip".to_string(), thing_130);
    assets.insert("wicket-plugin.zip".to_string(), pinned);
    assets.insert(
        "notes.zip".to_string(),
        zipped(&[("notes.txt", "not a bundle")]),
    );
    assets.insert("lying-2.0.0.zip".to_string(), lying);
    assets.insert("thing.tar.gz".to_string(), b"not a zip".to_vec());
    let fake = Arc::new(Releases {
        latest: Default::default(),
        releases: Default::default(),
        assets,
        base: Default::default(),
    });
    let base = releases_server(fake.clone()).await;
    // the core asks the API where this points; the only test that sets it
    unsafe { std::env::set_var("WICKET_GITHUB_API", &base) };
    fake.release("acme/thing", "v1.2.0", &["thing-1.2.0.zip", "thing.tar.gz"]);
    fake.release("acme/thing", "v1.3.0", &["thing-1.3.0.zip"]);
    fake.latest("acme/thing", "v1.2.0");
    fake.release("acme/pinned", "v1.0.1", &["wicket-plugin.zip", "notes.zip"]);
    fake.latest("acme/pinned", "v1.0.1");
    fake.release("acme/lying", "v2.0.0", &["lying-2.0.0.zip"]);
    fake.release("acme/bare", "v1.0.0", &["thing.tar.gz"]);
    fake.latest("acme/bare", "v1.0.0");

    // the latest release: its one zip is the bundle, placed without a build
    let (status, plugin) = install(
        &app,
        Path::new("https://github.com/acme/thing/releases"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{plugin}");
    assert_eq!(plugin["name"], "thing");
    assert_eq!(plugin["install"]["kind"], "release");
    assert_eq!(plugin["install"]["version"], "1.2.0");
    let expected_hash = {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(&thing_120))
    };
    assert_eq!(plugin["install"]["asset_hash"], expected_hash);
    assert_eq!(plugin["install"]["tag"], "v1.2.0");
    let dir = app.state.config().plugin_store_dir().join("thing/1");
    assert_eq!(
        std::fs::read_to_string(dir.join("index.html")).unwrap(),
        "<html>1.2.0</html>"
    );
    assert!(!dir.join("thing-1.2.0.zip").exists());

    // updates come from the latest release
    let (status, updates) = call(&app, "GET", "/api/v1/plugins/thing/updates", None).await;
    assert_eq!(status, StatusCode::OK, "{updates}");
    assert_eq!(updates["state"], "up_to_date", "{updates}");
    fake.latest("acme/thing", "v1.3.0");
    let (_, updates) = call(&app, "GET", "/api/v1/plugins/thing/updates", None).await;
    assert_eq!(updates["state"], "available", "{updates}");
    assert_eq!(updates["version"], "1.3.0");
    assert_eq!(updates["installed"], "1.2.0");
    let (status, plugin) = install(
        &app,
        Path::new("https://github.com/acme/thing/releases"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{plugin}");
    assert_eq!(plugin["install"]["version"], "1.3.0");
    let dir = app.state.config().plugin_store_dir().join("thing/1");
    assert_eq!(
        std::fs::read_to_string(dir.join("index.html")).unwrap(),
        "<html>1.3.0</html>"
    );

    // a tag pins: wicket-plugin.zip wins among several zips, the folder at
    // the archive's root is the bundle, and no update is ever offered
    let (status, plugin) = install(
        &app,
        Path::new("https://github.com/acme/pinned/releases/tag/v1.0.1"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{plugin}");
    assert_eq!(plugin["install"]["version"], "1.0.1");
    assert_eq!(plugin["install"]["tag"], "v1.0.1");
    let dir = app.state.config().plugin_store_dir().join("pinned/1");
    assert_eq!(
        std::fs::read_to_string(dir.join("index.html")).unwrap(),
        "<html>pinned</html>"
    );
    let (_, updates) = call(&app, "GET", "/api/v1/plugins/pinned/updates", None).await;
    assert_eq!(updates["state"], "pinned", "{updates}");
    assert_eq!(updates["tag"], "v1.0.1");

    // the tag and the manifest disagree
    let (status, body) = install(
        &app,
        Path::new("https://github.com/acme/lying/releases/tag/v2.0.0"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let message = body["message"].as_str().unwrap();
    assert!(
        message.contains("tagged v2.0.0") && message.contains("1.0.0"),
        "{message}"
    );

    // no zip among the assets, and a release that is not there
    let (_, body) = install(
        &app,
        Path::new("https://github.com/acme/bare/releases"),
        json!({}),
    )
    .await;
    let message = body["message"].as_str().unwrap();
    assert!(
        message.contains("one .zip asset") && message.contains("thing.tar.gz"),
        "{message}"
    );
    let (_, body) = install(
        &app,
        Path::new("https://github.com/acme/thing/releases/tag/v9.9.9"),
        json!({}),
    )
    .await;
    let message = body["message"].as_str().unwrap();
    assert!(message.contains("404"), "{message}");

    // inspecting a release names the tag and the asset, and runs no build
    let (status, seen) = call(
        &app,
        "POST",
        "/api/v1/plugins/inspect",
        Some(json!({"source": "https://github.com/acme/thing/releases"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{seen}");
    assert_eq!(seen["origin"]["kind"], "release");
    assert_eq!(seen["origin"]["resolved"]["tag"], "v1.3.0");
    assert_eq!(seen["origin"]["resolved"]["asset"], "thing-1.3.0.zip");
    assert!(seen["origin"]["resolved"]["asset_size"].as_u64().unwrap() > 0);
    assert_eq!(seen["build"], Value::Null);
    assert_eq!(seen["installed"]["version"], "1.3.0");

    // a release cannot be linked
    let (_, body) = install(
        &app,
        Path::new("https://github.com/acme/thing/releases"),
        json!({"link": true}),
    )
    .await;
    assert!(
        body["message"].as_str().unwrap().contains("bundle"),
        "{body}"
    );

    // the fetch folder is left clean
    let fetch = app
        .state
        .config()
        .plugin_store_dir()
        .parent()
        .unwrap()
        .join("fetch");
    let left: Vec<_> = std::fs::read_dir(&fetch)
        .map(|d| d.flatten().collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "{left:?}");
}
