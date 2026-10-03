//! The API end to end over its router. The envelope's fields and the exact
//! wording and order of violations, which plugins render, are checked against
//! the recorded responses in `tests/fixtures/api`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use pinrail_core::Config;
use pinrail_core::Pinrail;
use pinrail_core::api::router;
use pinrail_format::bundle::{Listing, Taken};
use serde_json::{Value, json};
use tower::ServiceExt;

mod common;
use common::{App, app, app_with, call, db};

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
        "payload": list_payload()
    })
}

/// An expiry past the year 9999 in UTC is refused: written as the store
/// writes it, it would sort before every other time and read as expired.
#[tokio::test]
async fn an_expiry_past_the_year_9999_is_refused() {
    let app = app();
    let mut body = submission();
    body["expires_at"] = json!("9999-12-31T23:59:59-01:00");
    let (status, refused) = call(&app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(violations(&refused)[0].0, "/expires_at");
}

/// The plugin derives a review's summary from its payload; an agent that
/// still sends one is told so, rather than having it dropped unseen.
#[tokio::test]
async fn a_summary_from_the_agent_is_refused() {
    let app = app();
    let mut body = submission();
    body["summary"] = json!({"counts": [["major", 1]]});
    let (status, refused) = call(&app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(
        violations(&refused),
        vec![(
            "/summary".to_string(),
            "summary is not accepted; the plugin derives it from the payload".to_string()
        )]
    );
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
    // the recorded answer holds exactly the fields a review has: one added
    // or dropped is a deliberate change of the fixture, which
    // `UPDATE_FIXTURES=1 cargo test` writes
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/api/create-ok.txt");
        std::fs::write(&path, format!("{review}\n201\n")).unwrap();
    }
    let (_, recorded) = fixture("create-ok");
    let fields = |v: &Value| {
        v.as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        fields(&review),
        fields(&recorded),
        "the envelope changed; run with UPDATE_FIXTURES=1 if that is meant"
    );
    assert!(review["id"].as_str().unwrap().starts_with("r_"));
    assert_eq!(review["plugin"], "list");
    assert_eq!(review["plugin_version"], "1.0.0");
    assert!(review["plugin_bundle"].is_string());
    assert_eq!(review["status"], "pending");
    assert_eq!(
        review["origin"],
        json!({"repo": "acme", "workflow": "review", "ref": "42", "run_id": "r1", "url": "https://x/42"})
    );
    assert_eq!(review["requested_by"], "agent");
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

    // the app shows origin.url as a link, so it must be a web address
    let with_url = |url: &str| {
        let mut body = submission();
        body["origin"]["url"] = json!(url);
        body
    };
    let (status, response) = call(
        &app,
        "POST",
        "/api/v1/reviews",
        Some(with_url("javascript:alert(1)")),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(
        violations(&response),
        vec![(
            "/origin/url".to_string(),
            "must be an http or https URL".to_string()
        )]
    );
    let (status, response) = call(
        &app,
        "POST",
        "/api/v1/reviews",
        Some(with_url("https://github.com/acme/api/pull/42")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{response}");
}

/// A review is summed up as its plugin declares: the request from the
/// payload when it is submitted, the outcome from the decision when it is
/// decided, both kept with the review.
#[tokio::test]
async fn a_review_is_summed_up_as_its_plugin_declares() {
    let app = app();
    let dir = app.dir.path().join("counted");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/list"),
        &dir,
    );
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    manifest["name"] = json!("counted");
    manifest["summary"] = json!({
        "request": {"counts": [{
            "items": "/groups/*/items", "by": "severity",
            "values": {"major": {"tone": "warning"}, "minor": {"tone": "info"}}
        }]},
        "outcome": {"counts": [
            {"items": "/decisions", "by": "action", "values": {
                "accept": {"label": "accepted", "tone": "success"},
                "reject": {"label": "rejected", "tone": "danger"}
            }},
            {"items": "/undecided", "label": "undecided"}
        ]}
    });
    std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
    let (status, body) = install(&app, &dir, json!({"link": true})).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let mut body = submission();
    body["plugin"] = json!("counted");
    let review = submit(&app, body).await;
    assert_eq!(
        review["summary"],
        json!({"counts": [
            {"label": "major", "count": 1, "tone": "warning"},
            {"label": "minor", "count": 1, "tone": "info"}
        ]})
    );

    let id = review["id"].as_str().unwrap();
    let (status, decided) = call(
        &app,
        "POST",
        &format!("/api/v1/reviews/{id}/decision"),
        Some(json!({"data": {"decisions": [{"id": 1, "action": "accept"}], "undecided": [2]}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    let expected = json!({"counts": [
        {"label": "accepted", "count": 1, "tone": "success"},
        {"label": "undecided", "count": 1, "tone": "neutral"}
    ]});
    assert_eq!(decided["decision"]["summary"], expected);
    // read back from the store, as a listing or a restart sees it
    let (_, stored) = call(&app, "GET", &format!("/api/v1/reviews/{id}"), None).await;
    assert_eq!(stored["summary"]["counts"][0]["label"], "major");
    assert_eq!(stored["decision"]["summary"], expected);
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
    // not the payload, which can be large and would match on anything
    let (_, payload) = call(&app, "GET", "/api/v1/reviews?q=dedups", None).await;
    assert_eq!(payload["total"], 0, "{payload}");

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
async fn plugins_are_listed_installed_one_by_one_and_reloaded() {
    let app = app();
    let names = |body: &Value| -> Vec<String> {
        body["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap().to_string())
            .collect()
    };
    let (status, body) = call(&app, "GET", "/api/v1/plugins", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        names(&body),
        vec!["feedback", "list"],
        "a fresh app has the ones it ships"
    );
    assert_eq!(body["plugins"][0]["usable"], true);

    // one plugin, named by its own folder, served live from where it sits
    let samples: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let (status, body) = install(&app, &samples.join("hello"), json!({"link": true})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, body) = call(&app, "GET", "/api/v1/plugins", None).await;
    assert_eq!(names(&body), vec!["feedback", "hello", "list"]);

    let links: Vec<_> = db(&app)
        .installs()
        .unwrap()
        .into_iter()
        .filter(|r| r.kind != "app")
        .collect();
    assert_eq!(links.len(), 1, "one install, one record");
    assert!(links.iter().all(|r| r.linked() && r.name == "hello"));
    let hello = body["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "hello")
        .unwrap();
    assert_eq!(hello["install"]["link"], true);
    assert_eq!(hello["install"]["source_kind"], "folder");
    assert_eq!(hello["version"], "1.0.0");
    assert!(
        body["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "list")
            .unwrap()["install"]["source_kind"]
            == "app",
        "the official one comes with the app"
    );

    // a source that is not there is refused, naming the field
    let (status, body) = install(&app, Path::new("/nope/nowhere"), Value::Null).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("/source: /nope/nowhere is not a directory"),
        "{body}"
    );

    let (status, body) = call(&app, "POST", "/api/v1/plugins/reload", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 3, "the two built-in ones and hello");

    // a plugin named in a path by its name; a publisher is no part of it
    for name in ["nope", "forgeplane%2Flist"] {
        let (status, _) = call(
            &app,
            "GET",
            &format!("/api/v1/plugins/{name}/describe"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{name}");
    }
    for name in ["list"] {
        let (status, body) = call(
            &app,
            "GET",
            &format!("/api/v1/plugins/{name}/describe"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{name}");
        assert_eq!(body["plugins"][0]["name"], "list");
    }
}

/// A review's frame loads the bundle the review was submitted to, at an
/// address that names the bundle, under the sandbox's policy.
#[tokio::test]
async fn a_review_is_shown_from_its_bundle_with_the_sandbox_csp() {
    let app = app();
    let review = submit(&app, submission()).await;
    assert_eq!(review["plugin"], "list");
    assert_eq!(review["plugin_version"], "1.0.0");
    let id = review["id"].as_str().unwrap();
    let (status, view) = call(&app, "GET", &format!("/api/v1/reviews/{id}/view"), None).await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["plugin"]["name"], "list");
    assert_eq!(view["plugin"]["version"], "1.0.0");
    let bundle = current_bundle(&app, "list").unwrap();
    assert_eq!(view["url"], format!("/bundles/{bundle}/view/index.html"));

    let response = bundle_get(&app, view["url"].as_str().unwrap(), &[]).await;
    assert_eq!(response.status(), StatusCode::OK);
    let csp = response.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .to_string();
    let base = format!("http://127.0.0.1:4747/bundles/{bundle}/view/");
    assert!(
        csp.starts_with(&format!(
            "sandbox allow-scripts; default-src 'none'; script-src 'unsafe-inline' {base} http://127.0.0.1:4747/sdk/"
        )),
        "{csp}"
    );
    // the SDK stylesheet names a typeface the SDK itself serves
    assert!(
        csp.contains(&format!("font-src data: {base} http://127.0.0.1:4747/sdk/")),
        "{csp}"
    );
    assert!(
        csp.ends_with(
            "connect-src 'none'; form-action 'none'; base-uri 'none'; frame-ancestors 'self' tauri://localhost http://tauri.localhost http://localhost:5173"
        ),
        "{csp}"
    );

    // no address by name and version, whatever is current
    for gone in [
        "/plugins/list/1/view/index.html",
        "/plugins/forgeplane/list/1/view/index.html",
    ] {
        let response = bundle_get(&app, gone, &[]).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{gone}");
    }
    let (status, _) = call(&app, "GET", "/api/v1/reviews/r_nope/view", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_sdk_is_served_only_when_configured() {
    let app = app();
    let (status, _) = call(&app, "GET", "/sdk/v1/pinrail-plugin.js", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("pinrail-plugin.js"), "export const ok = 1;").unwrap();
    let data = tempfile::tempdir().unwrap();
    let mut config = Config::new(data.path(), 0);
    config.sdk_dir = Some(dir.path().to_path_buf());
    let state = Arc::new(Pinrail::open(config).unwrap());
    let router = router(state);
    let response = router
        .clone()
        .oneshot(
            Request::get("/sdk/v1/pinrail-plugin.js")
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

    // a developer's SDK folder may hold anything: a page in it opened in a
    // browser runs sandboxed, and hidden files are not served
    std::fs::write(dir.path().join("page.html"), "<script>1</script>").unwrap();
    std::fs::write(dir.path().join(".env"), "TOKEN=1").unwrap();
    let get = |path: &str| {
        router
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
    };
    let page = get("/sdk/v1/page.html").await.unwrap();
    assert!(
        page.headers()
            .get("content-security-policy")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|csp| csp.contains("sandbox")),
        "{:?}",
        page.headers()
    );
    assert_eq!(
        get("/sdk/v1/.env").await.unwrap().status(),
        StatusCode::NOT_FOUND
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
            "

> ship it

## lib/acme/tickets.ex

- **#1 accepted** — do_save dedups without reversing (major)
- **#2 rejected** — moduledoc typo (minor)
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
    assert_eq!(info["version"], pinrail_core::VERSION);
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

/// A request as a browser or another program would send it, header by header.
/// A removed plugin's permission to open links goes with it, so a plugin
/// installed later under the same name starts without it.
#[tokio::test]
async fn removing_a_plugin_forgets_the_links_it_was_allowed_to_open() {
    let app = app();
    let root = tempfile::tempdir().unwrap();
    let hello = plugin_copy(root.path(), "hello", "1.0.0");
    let (status, row) = install(&app, &hello, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    let links = json!({"links": {"hello": {"source": row["install"]["source"], "origins": ["https://github.com"]}}});
    let (status, body) = call(&app, "PATCH", "/api/v1/settings", Some(links)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = call(&app, "DELETE", "/api/v1/plugins/hello", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, settings) = call(&app, "GET", "/api/v1/settings", None).await;
    assert_eq!(settings["links"], json!({}), "{settings}");
}

/// The install dialog shows the plugin it is about to install with its
/// icon, as the app shows an installed one.
#[tokio::test]
async fn an_inspection_shows_the_plugins_icon() {
    let app = app();
    let root = tempfile::tempdir().unwrap();
    let hello = plugin_copy(root.path(), "hello", "1.0.0");
    let (status, seen) = call(
        &app,
        "POST",
        "/api/v1/plugins/inspect",
        Some(json!({"source": hello.display().to_string()})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{seen}");
    assert!(
        seen["icon"]
            .as_str()
            .is_some_and(|svg| svg.contains("<svg")),
        "{seen}"
    );
}

/// A linked plugin is served from the developer's own folder, which holds
/// more than a plugin: only what an installed copy would hold is served.
#[tokio::test]
async fn a_linked_plugin_serves_only_what_an_installed_copy_would_hold() {
    let app = app();
    let root = tempfile::tempdir().unwrap();
    let hello = plugin_copy(root.path(), "hello", "1.0.0");
    for (file, text) in [
        (".env", "API_KEY=secret"),
        (".git/config", "[remote] url = https://token@example.com"),
        ("node_modules/lib/index.js", "x"),
        ("src/main.ts", "x"),
        ("package.json", "{}"),
        ("view/.secret", "x"),
        ("notes.txt", "x"),
        ("cache/build.bin", "x"),
    ] {
        let path = hello.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let (status, row) = install(&app, &hello, json!({"link": true})).await;
    assert_eq!(status, StatusCode::OK, "{row}");

    let host = [("host", "127.0.0.1:4747")];
    let (status, _) = raw(&app, "GET", "/links/hello/view/index.html", &host, "").await;
    assert_eq!(status, StatusCode::OK);
    for file in [
        "manifest.json",
        "schemas/payload.schema.json",
        ".env",
        ".git/config",
        "node_modules/lib/index.js",
        "src/main.ts",
        "package.json",
        "view/.secret",
        "notes.txt",
        "cache/build.bin",
    ] {
        let (status, body) = raw(&app, "GET", &format!("/links/hello/{file}"), &host, "").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{file}: {body}");
    }
}

/// The bundle an installed plugin's new reviews use.
fn current_bundle(app: &App, plugin: &str) -> Option<String> {
    db(app).install(plugin).unwrap().and_then(|i| i.bundle)
}

/// A request for a file of a stored bundle, as a view's frame makes it.
async fn bundle_get(app: &App, uri: &str, extra: &[(&str, &str)]) -> axum::http::Response<Body> {
    let mut request = Request::get(uri).header("host", "127.0.0.1:4747");
    for (name, value) in extra {
        request = request.header(*name, *value);
    }
    app.router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

/// A stored bundle serves its view's files, and only those, for ever: the
/// address names the bundle, which never changes.
#[tokio::test]
async fn a_stored_bundle_serves_its_view_and_nothing_else() {
    let app = app();
    let root = tempfile::tempdir().unwrap();
    let hello = plugin_copy(root.path(), "hello", "1.0.0");
    let stored = app.state.bundles().store(&hello).unwrap();
    let hash = stored.hash;
    let listing = Listing::of_folder(&hello, Taken::FromSource).unwrap();
    let page = listing.file("view/index.html").unwrap().sha256.clone();

    let response = bundle_get(&app, &format!("/bundles/{hash}/view/index.html"), &[]).await;
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers().clone();
    assert_eq!(headers["content-type"], "text/html");
    assert_eq!(
        headers["cache-control"],
        "public, max-age=31536000, immutable"
    );
    assert_eq!(headers["etag"], format!("\"{page}\""));
    assert_eq!(headers["x-content-type-options"], "nosniff");
    let csp = headers["content-security-policy"].to_str().unwrap();
    let base = format!("http://127.0.0.1:4747/bundles/{hash}/view/");
    assert!(
        csp.starts_with(&format!(
            "sandbox allow-scripts; default-src 'none'; script-src 'unsafe-inline' {base} http://127.0.0.1:4747/sdk/"
        )),
        "{csp}"
    );
    assert!(csp.contains("connect-src 'none'"), "{csp}");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body, std::fs::read(hello.join("view/index.html")).unwrap());

    // the same file again, as the webview asks for one it has
    let again = bundle_get(
        &app,
        &format!("/bundles/{hash}/view/index.html"),
        &[("if-none-match", &format!("\"{page}\""))],
    )
    .await;
    assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(again.headers()["etag"], format!("\"{page}\""));

    // a file of the view's own folder, with its type
    let icon = bundle_get(&app, &format!("/bundles/{hash}/view/icons/check.svg"), &[]).await;
    assert_eq!(icon.status(), StatusCode::OK);
    assert_eq!(icon.headers()["content-type"], "image/svg+xml");

    // nothing outside view/, whatever the address
    for uri in [
        format!("/bundles/{hash}/manifest.json"),
        format!("/bundles/{hash}/schemas/payload.schema.json"),
        format!("/bundles/{hash}/samples/hello.json"),
        format!("/bundles/{hash}/view/../manifest.json"),
        format!("/bundles/{hash}/view/%2e%2e/manifest.json"),
        format!("/bundles/{hash}/view/missing.js"),
        format!("/bundles/{}/view/index.html", "0".repeat(64)),
        "/bundles/not-a-hash/view/index.html".to_string(),
    ] {
        let response = bundle_get(&app, &uri, &[]).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }

    // a fetch comes from another page, reading what is not its own
    let fetched = bundle_get(
        &app,
        &format!("/bundles/{hash}/view/index.html"),
        &[("sec-fetch-dest", "empty")],
    )
    .await;
    assert_eq!(fetched.status(), StatusCode::FORBIDDEN);
}

/// A stored file that no longer matches its bundle's listing is not served:
/// it is not the release the address names.
#[cfg(unix)]
#[tokio::test]
async fn a_changed_bundle_file_is_not_served() {
    use std::os::unix::fs::PermissionsExt;
    let app = app();
    let root = tempfile::tempdir().unwrap();
    let hello = plugin_copy(root.path(), "hello", "1.0.0");
    let hash = app.state.bundles().store(&hello).unwrap().hash;
    let page = app.state.bundles().path(&hash).join("view/index.html");
    std::fs::set_permissions(&page, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::write(&page, "<script>changed</script>").unwrap();

    let response = bundle_get(&app, &format!("/bundles/{hash}/view/index.html"), &[]).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(!String::from_utf8_lossy(&body).contains("changed"));
}

/// A plugin's files are for its own view, which loads them as a page,
/// scripts, styles, fonts and images from an opaque origin: another
/// website cannot read them.
#[tokio::test]
async fn other_websites_cannot_read_a_plugins_files() {
    let app = app();
    let bundle = current_bundle(&app, "list").unwrap();
    let request = |dest: Option<&str>| {
        let mut r = Request::get(format!("/bundles/{bundle}/view/index.html"))
            .header("host", "127.0.0.1:4747")
            .header("origin", "https://evil.example");
        if let Some(dest) = dest {
            r = r.header("sec-fetch-dest", dest);
        }
        r.body(Body::empty()).unwrap()
    };
    // a frame loading the view: served, readable only by an opaque origin
    let response = app
        .router
        .clone()
        .oneshot(request(Some("iframe")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["access-control-allow-origin"], "null");
    // a script's fetch, which a view cannot make: refused
    let response = app
        .router
        .clone()
        .oneshot(request(Some("empty")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// A symbolic link in a plugin could point anywhere on the machine, such
/// as a private key: an installed copy never contains one, and a linked
/// plugin never serves a file from outside its folder.
#[cfg(unix)]
#[tokio::test]
async fn a_plugin_never_brings_in_files_from_outside_its_folder() {
    let app = app();
    let root = tempfile::tempdir().unwrap();
    let secret = root.path().join("secret.txt");
    std::fs::write(&secret, "PRIVATE KEY").unwrap();
    let outside_dir = root.path().join("outside");
    std::fs::create_dir(&outside_dir).unwrap();
    std::fs::write(outside_dir.join("key"), "PRIVATE KEY").unwrap();

    // installing a copy: refused, whether the link is to a file or a folder
    for (name, link, target) in [
        ("filelink", "view/notes.txt", &secret),
        ("dirlink", "view/assets", &outside_dir),
    ] {
        let plugin = plugin_copy(root.path(), "hello", "1.0.0");
        let renamed = root.path().join(name);
        std::fs::rename(&plugin, &renamed).unwrap();
        std::os::unix::fs::symlink(target, renamed.join(link)).unwrap();
        let (status, body) = install(&app, &renamed, json!({})).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{name}: {body}");
        assert!(
            body["message"]
                .as_str()
                .unwrap_or_default()
                .contains("symbolic link"),
            "{name}: {body}"
        );
    }

    // linking the folder: the file outside is not served, a link inside is
    let linked = plugin_copy(root.path(), "hello", "1.0.0");
    std::os::unix::fs::symlink(&secret, linked.join("view/notes.txt")).unwrap();
    std::os::unix::fs::symlink(
        linked.join("view/index.html"),
        linked.join("view/alias.html"),
    )
    .unwrap();
    let (status, body) = install(&app, &linked, json!({"link": true})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let host = [("host", "127.0.0.1:4747")];
    let (status, body) = raw(&app, "GET", "/links/hello/view/notes.txt", &host, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, _) = raw(&app, "GET", "/links/hello/view/alias.html", &host, "").await;
    assert_eq!(status, StatusCode::OK);
}

/// Rounds form one line: a new round revises the latest round of its review,
/// with the same plugin, and a round still pending when it is revised is
/// withdrawn, so its waiting agent learns it was superseded.
#[tokio::test]
async fn a_new_round_revises_the_latest_round_with_the_same_plugin() {
    let app = app();
    let first = submit(&app, submission()).await;
    let first_id = first["id"].as_str().unwrap().to_string();

    // the first round is still pending: the second supersedes it
    let mut body = submission();
    body["revises"] = json!(first_id);
    let second = submit(&app, body).await;
    let second_id = second["id"].as_str().unwrap().to_string();
    let (_, first) = call(&app, "GET", &format!("/api/v1/reviews/{first_id}"), None).await;
    assert_eq!(first["status"], "withdrawn", "{first}");
    assert_eq!(
        first["withdrawn_reason"],
        format!("superseded by {second_id}"),
        "{first}"
    );

    // the first round already has a newer one: revise that instead
    let mut body = submission();
    body["revises"] = json!(first_id);
    let (status, refused) = call(&app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(violations(&refused)[0].0, "/revises");
    assert!(violations(&refused)[0].1.contains(&second_id), "{refused}");

    // another plugin's review is not an earlier round of this one
    let hello = json!({"plugin": "feedback", "title": "Other", "revises": second_id, "payload": {"groups": [{"id": "g", "title": "G", "questions": [{"id": "q", "type": "text", "prompt": "Why?"}]}]}});
    let (status, refused) = call(&app, "POST", "/api/v1/reviews", Some(hello)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(violations(&refused)[0].0, "/revises");
    assert!(
        violations(&refused)[0].1.contains("same plugin"),
        "{refused}"
    );
}

/// An agent that sends the same review again while the first is still
/// waiting, as one that gives up on its command and runs it again does, gets
/// the review it already has, not a second copy for the person to decide.
#[tokio::test]
async fn the_same_submission_while_the_first_is_pending_is_one_review() {
    let app = app();
    let (status, first) = call(&app, "POST", "/api/v1/reviews", Some(submission())).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let (status, again) = call(&app, "POST", "/api/v1/reviews", Some(submission())).await;
    assert_eq!(
        again["id"], first["id"],
        "a second review was created: {again}"
    );
    assert_eq!(status, StatusCode::OK);
    let (_, pending) = call(&app, "GET", "/api/v1/reviews?status=pending", None).await;
    assert_eq!(pending["total"], 1, "{pending}");
}

/// A linked plugin is looked up by its full name, never joined into a
/// path: a folder beside the plugins' data is not reached through `..`,
/// and only an installed link is served.
#[tokio::test]
async fn a_link_is_served_only_for_a_linked_plugins_own_name() {
    let app = app();
    let outside = app.state.config().plugins_dir().join("outside");
    std::fs::create_dir_all(outside.join("view")).unwrap();
    std::fs::write(outside.join("view/index.html"), "<html>outside</html>").unwrap();
    for path in [
        "/links/..%2Foutside/x/view/index.html",
        "/links/..%2F..%2Fplugins/outside/view/index.html",
        "/links/outside/view/index.html",
        // the official plugins are bundles, not links
        "/links/list/view/index.html",
    ] {
        let response = bundle_get(&app, path, &[]).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
}

async fn raw(
    app: &App,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(path);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = app
        .router
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()));
    (status, value)
}

#[tokio::test]
async fn a_page_that_rebound_its_name_to_loopback_is_refused() {
    let app = app();
    submit(&app, submission()).await;
    // what a page on evil.example sends once its name resolves to 127.0.0.1
    for path in [
        "/api/v1/info",
        "/api/v1/reviews",
        "/bundles/0/view/index.html",
        "/api/v1/events",
    ] {
        let (status, body) = raw(&app, "GET", path, &[("host", "evil.example:4747")], "").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(body["error"], "forbidden_host", "{path}");
        assert!(
            body["message"]
                .as_str()
                .unwrap()
                .contains("evil.example:4747"),
            "{body}"
        );
    }
    let (status, _) = raw(
        &app,
        "POST",
        "/api/v1/reviews",
        &[
            ("host", "evil.example:4747"),
            ("content-type", "application/json"),
        ],
        &submission().to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        call(&app, "GET", "/api/v1/reviews", None).await.1["reviews"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "nothing was created"
    );

    // the names the server does answer to, and a program that sends none
    for host in [
        "127.0.0.1:4747",
        "localhost:4747",
        "[::1]:4747",
        "127.0.0.1",
    ] {
        let (status, _) = raw(&app, "GET", "/api/v1/info", &[("host", host)], "").await;
        assert_eq!(status, StatusCode::OK, "{host}");
    }
    let (status, _) = raw(&app, "GET", "/api/v1/info", &[], "").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_write_that_does_not_say_it_is_json_is_refused() {
    let app = app();
    let id = submit(&app, submission()).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let host = ("host", "127.0.0.1:4747");
    let page = ("origin", "https://evil.example");
    let body = submission().to_string();
    // what a page on any site may send cross-origin without asking first
    for content_type in [
        "text/plain",
        "application/x-www-form-urlencoded",
        "multipart/form-data; boundary=x",
    ] {
        let (status, refused) = raw(
            &app,
            "POST",
            "/api/v1/reviews",
            &[host, page, ("content-type", content_type)],
            &body,
        )
        .await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{content_type}");
        assert_eq!(refused["error"], "unsupported_media_type");
    }
    // no body and no type at all: withdraw and discard act on nothing more
    for action in ["withdraw", "discard", "viewed"] {
        let (status, _) = raw(
            &app,
            "POST",
            &format!("/api/v1/reviews/{id}/{action}"),
            &[host, page],
            "",
        )
        .await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{action}");
    }
    let (status, _) = raw(
        &app,
        "PATCH",
        "/api/v1/settings",
        &[host, ("content-type", "text/plain")],
        "{}",
    )
    .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let (_, listing) = call(&app, "GET", "/api/v1/reviews", None).await;
    assert_eq!(
        listing["reviews"].as_array().unwrap().len(),
        1,
        "nothing was created"
    );
    assert_eq!(
        listing["reviews"][0]["status"], "pending",
        "nothing was withdrawn or discarded"
    );

    // JSON needs a preflight, and a page's origin is not granted one
    let preflight = Request::builder()
        .method("OPTIONS")
        .uri("/api/v1/reviews")
        .header("host", "127.0.0.1:4747")
        .header("origin", "https://evil.example")
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "content-type")
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(preflight).await.unwrap();
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );

    // the same writes, saying they are JSON, go through; a charset is fine
    let (status, _) = raw(
        &app,
        "POST",
        &format!("/api/v1/reviews/{id}/viewed"),
        &[host, ("content-type", "application/json")],
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = raw(
        &app,
        "POST",
        "/api/v1/reviews/validate",
        &[host, ("content-type", "application/json; charset=utf-8")],
        &body,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_body_over_the_limit_is_refused_in_json() {
    let app = app();
    let json = [
        ("host", "127.0.0.1:4747"),
        ("content-type", "application/json"),
    ];
    let sized = |bytes: usize| {
        let mut body = submission();
        let room = bytes - body.to_string().len() - r#","filler":"""#.len();
        body["filler"] = Value::String("x".repeat(room));
        let text = body.to_string();
        assert_eq!(text.len(), bytes);
        text
    };
    let over = sized(4 * 1024 * 1024 + 1);
    let length = over.len().to_string();
    let mut headers = json.to_vec();
    headers.push(("content-length", &length));
    let (status, refused) = raw(&app, "POST", "/api/v1/reviews", &headers, &over).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(refused["error"], "too_large");
    assert_eq!(refused["message"], "the body is 4.0 MB; the limit is 4 MB");
    let (_, listing) = call(&app, "GET", "/api/v1/reviews", None).await;
    assert!(
        listing["reviews"].as_array().unwrap().is_empty(),
        "nothing was created"
    );

    // just under the limit gets past it, to the envelope's own checks
    let (status, _) = raw(
        &app,
        "POST",
        "/api/v1/reviews/validate",
        &json,
        &sized(4 * 1024 * 1024),
    )
    .await;
    assert_ne!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(bytes))
}

/// A PUT of raw bytes, as the CLI uploads an attachment.
async fn put_bytes(
    app: &App,
    path: &str,
    content_type: &str,
    bytes: Vec<u8>,
    length: bool,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("PUT")
        .uri(path)
        .header("host", "127.0.0.1:4747")
        .header("content-type", content_type);
    if length {
        request = request.header("content-length", bytes.len().to_string());
    }
    let response = app
        .router
        .clone()
        .oneshot(request.body(Body::from(bytes)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

async fn head(app: &App, path: &str) -> (StatusCode, Option<String>) {
    let request = Request::builder()
        .method("HEAD")
        .uri(path)
        .header("host", "127.0.0.1:4747")
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let length = response
        .headers()
        .get("content-length")
        .map(|v| v.to_str().unwrap().to_string());
    (response.status(), length)
}

#[tokio::test]
async fn an_attachment_is_uploaded_once_by_its_hash() {
    let app = app();
    let bytes = b"glTF, more or less".to_vec();
    let hash = sha256(&bytes);
    let path = format!("/api/v1/attachments/{hash}");
    assert_eq!(head(&app, &path).await.0, StatusCode::NOT_FOUND);

    let (status, stored) =
        put_bytes(&app, &path, "application/octet-stream", bytes.clone(), true).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(stored, json!({ "sha256": hash, "size": 18 }));
    assert_eq!(head(&app, &path).await, (StatusCode::OK, Some("18".into())));
    let file = app
        .state
        .config()
        .attachments_dir()
        .join("sha256")
        .join(&hash[..2])
        .join(&hash);
    assert_eq!(std::fs::read(file).unwrap(), bytes);

    // again: already there, nothing to do
    let (status, again) = put_bytes(&app, &path, "application/octet-stream", bytes, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again, stored);
}

#[tokio::test]
async fn an_upload_is_refused_when_it_is_not_what_it_says() {
    let app = app();
    let hash = sha256(b"what was promised");
    let path = format!("/api/v1/attachments/{hash}");
    let (status, refused) = put_bytes(
        &app,
        &path,
        "application/octet-stream",
        b"something else".to_vec(),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        refused["violations"][0]["message"]
            .as_str()
            .unwrap()
            .contains(&sha256(b"something else")),
        "{refused}"
    );
    assert_eq!(head(&app, &path).await.0, StatusCode::NOT_FOUND);

    // only raw bytes, which a web page cannot send across origins unasked
    for content_type in [
        "text/plain",
        "application/json",
        "multipart/form-data; boundary=x",
    ] {
        let (status, refused) = put_bytes(
            &app,
            &path,
            content_type,
            b"what was promised".to_vec(),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{content_type}");
        assert_eq!(refused["error"], "unsupported_media_type");
    }
    // a name that is not a hash
    for bad in ["abc", &"A".repeat(64)] {
        let (status, _) = put_bytes(
            &app,
            &format!("/api/v1/attachments/{bad}"),
            "application/octet-stream",
            b"x".to_vec(),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
        assert_eq!(
            head(&app, &format!("/api/v1/attachments/{bad}")).await.0,
            StatusCode::NOT_FOUND
        );
    }
}

#[tokio::test]
async fn an_upload_past_the_cap_is_refused_in_json_and_leaves_nothing() {
    let app = app_with(|c| c.max_attachment_bytes = 1024 * 1024);
    let bytes = vec![7u8; 1024 * 1024 + 1];
    let path = format!("/api/v1/attachments/{}", sha256(&bytes));
    // said up front, or found out on the way
    for length in [true, false] {
        let (status, refused) = put_bytes(
            &app,
            &path,
            "application/octet-stream",
            bytes.clone(),
            length,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::PAYLOAD_TOO_LARGE,
            "content-length: {length}"
        );
        assert_eq!(refused["error"], "too_large");
        assert_eq!(refused["message"], "an attachment may be 1 MB at most");
    }
    assert_eq!(head(&app, &path).await.0, StatusCode::NOT_FOUND);
    let tmp = app.state.config().attachments_dir().join("tmp");
    assert_eq!(std::fs::read_dir(tmp).unwrap().count(), 0);
}

#[tokio::test]
async fn an_upload_is_not_held_to_the_json_limit() {
    let app = app();
    let bytes = vec![3u8; 6 * 1024 * 1024];
    let (status, stored) = put_bytes(
        &app,
        &format!("/api/v1/attachments/{}", sha256(&bytes)),
        "application/octet-stream",
        bytes,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{stored}");
    assert_eq!(stored["size"], 6 * 1024 * 1024);
}

/// A plugin that takes models and images beside its payload, three at most,
/// installed as a link from `root`.
async fn files_plugin(app: &App, root: &Path, attachments: Value) -> Value {
    let dir = root.join("files");
    view(&dir, "<html></html>");
    schemas(
        &dir,
        json!({ "type": "object", "properties": { "files": { "type": "array" } } }),
    );
    std::fs::write(
        dir.join("manifest.json"),
        json!({
            "name": "files", "version": "1.0.0",
            "attachments": attachments,
        })
        .to_string(),
    )
    .unwrap();
    install(app, &dir, json!({ "link": true })).await.1
}

async fn upload(app: &App, bytes: &[u8]) -> String {
    let hash = sha256(bytes);
    let (status, _) = put_bytes(
        app,
        &format!("/api/v1/attachments/{hash}"),
        "application/octet-stream",
        bytes.to_vec(),
        true,
    )
    .await;
    assert!(status.is_success());
    hash
}

fn with_files(payload: Value, attachments: Value) -> Value {
    json!({ "plugin": "files", "title": "Two lamps", "payload": payload, "attachments": attachments })
}

#[tokio::test]
async fn a_review_carries_the_files_its_payload_names() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    files_plugin(
        &app,
        scratch.path(),
        json!({ "accept": [".glb", "image/*"], "max_count": 3 }),
    )
    .await;
    let pivot = upload(&app, b"pivot glb").await;
    let photo = upload(&app, b"a photo").await;
    let body = with_files(
        json!({ "files": [{ "$attachment": "pivot.glb" }, { "$attachment": "desk.jpg" }] }),
        json!({
            "pivot.glb": { "sha256": pivot, "size": 9, "media_type": "model/gltf-binary" },
            "desk.jpg": { "sha256": photo, "size": 7, "media_type": "image/jpeg" },
        }),
    );
    let (status, created) = call(&app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let review = db(&app)
        .get_review(created["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    let carried: Vec<(&str, &str, u64, &str)> = review
        .attachments
        .iter()
        .map(|a| {
            (
                a.name.as_str(),
                a.sha256.as_str(),
                a.size,
                a.media_type.as_str(),
            )
        })
        .collect();
    assert_eq!(
        carried,
        [
            ("desk.jpg", photo.as_str(), 7, "image/jpeg"),
            ("pivot.glb", pivot.as_str(), 9, "model/gltf-binary")
        ]
    );

    // a named blob outlives the sweep; once its review is swept, it goes
    let later = chrono::Utc::now() + chrono::Duration::hours(2);
    assert_eq!(app.state.attachments().sweep(later).unwrap(), 0);
    let (status, _) = call(
        &app,
        "POST",
        &format!(
            "/api/v1/reviews/{}/withdraw",
            created["id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    app.state.reviews().sweep_history_before(later).unwrap();
    assert_eq!(app.state.attachments().sweep(later).unwrap(), 2);
    assert!(app.state.attachments().stored(&pivot).unwrap().is_none());
    assert!(
        !app.state
            .config()
            .attachments_dir()
            .join("sha256")
            .join(&pivot[..2])
            .join(&pivot)
            .exists()
    );
}

#[tokio::test]
async fn a_dry_run_checks_the_files_before_they_are_uploaded() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    files_plugin(&app, scratch.path(), json!({ "accept": [".glb"] })).await;
    let hash = sha256(b"not sent yet");
    let body = with_files(
        json!({ "files": [{ "$attachment": "a.glb" }] }),
        json!({ "a.glb": { "sha256": hash, "size": 12 } }),
    );
    let (status, valid) = call(&app, "POST", "/api/v1/reviews/validate", Some(body.clone())).await;
    assert_eq!(status, StatusCode::OK, "{valid}");
    let (status, refused) = call(&app, "POST", "/api/v1/reviews", Some(body.clone())).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        violations(&refused),
        [(
            "/attachments/a.glb/sha256".to_string(),
            "not uploaded: PUT /api/v1/attachments/{sha256} first".to_string()
        )]
    );
    upload(&app, b"not sent yet").await;
    let (status, _) = call(&app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn a_file_confirmed_as_stored_outlasts_the_next_sweep() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    files_plugin(&app, scratch.path(), json!({ "accept": [".glb"] })).await;
    let bytes = b"uploaded long ago".to_vec();
    let hash = upload(&app, &bytes).await;
    // an upload nothing was submitted with, older than the sweep's hour
    let age = || {
        rusqlite::Connection::open(app.state.config().db_path())
            .unwrap()
            .execute("UPDATE blobs SET created_at = '2000-01-01T00:00:00Z'", [])
            .unwrap();
    };
    let hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);
    let path = format!("/api/v1/attachments/{hash}");

    // an agent uploads it again, is told it is already stored, and submits
    age();
    let (status, _) = put_bytes(&app, &path, "application/octet-stream", bytes, true).await;
    assert_eq!(status, StatusCode::OK);
    app.state.attachments().sweep(hour_ago).unwrap();
    assert_eq!(head(&app, &path).await.0, StatusCode::OK, "swept after PUT");

    // or asks whether it is there
    age();
    assert_eq!(head(&app, &path).await.0, StatusCode::OK);
    app.state.attachments().sweep(hour_ago).unwrap();
    assert_eq!(
        head(&app, &path).await.0,
        StatusCode::OK,
        "swept after HEAD"
    );

    let body = with_files(
        json!({ "files": [{ "$attachment": "a.glb" }] }),
        json!({ "a.glb": { "sha256": hash, "size": 17 } }),
    );
    let (status, created) = call(&app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
}

#[tokio::test]
async fn a_file_swept_while_its_review_is_saved_is_reported_as_not_uploaded() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    files_plugin(&app, scratch.path(), json!({ "accept": [".glb"] })).await;
    let hash = upload(&app, b"swept in between").await;
    // the sweep, landing after the submission checked the file and before
    // the review's row for it is written
    rusqlite::Connection::open(app.state.config().db_path())
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER sweep BEFORE INSERT ON review_attachments
             BEGIN DELETE FROM blobs WHERE sha256 = NEW.sha256; END;",
        )
        .unwrap();
    let body = with_files(
        json!({ "files": [{ "$attachment": "a.glb" }] }),
        json!({ "a.glb": { "sha256": hash, "size": 16 } }),
    );
    let (status, refused) = call(&app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(
        violations(&refused),
        [(
            "/attachments/a.glb/sha256".to_string(),
            "not uploaded: PUT /api/v1/attachments/{sha256} first".to_string()
        )]
    );
}

#[tokio::test]
async fn files_a_plugin_does_not_take_or_a_payload_does_not_have_are_refused() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    files_plugin(
        &app,
        scratch.path(),
        json!({ "accept": [".glb", "image/*"], "max_count": 2 }),
    )
    .await;
    let glb = upload(&app, b"glb").await;

    // a plugin with no attachments block takes none
    let mut to_list = submission();
    to_list["attachments"] = json!({ "a.glb": { "sha256": glb, "size": 3 } });
    let (status, refused) = call(&app, "POST", "/api/v1/reviews", Some(to_list)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        violations(&refused),
        [(
            "/attachments".to_string(),
            "this plugin takes no attachments".to_string()
        )]
    );

    let body = with_files(
        json!({ "files": [{ "$attachment": "a.glb" }, { "$attachment": "missing.glb" }, { "$attachment": 7 }, "attachment:a.glb is text"] }),
        json!({
            "a.glb": { "sha256": glb, "size": 4 },
            "notes.pdf": { "sha256": glb, "size": 3, "media_type": "application/pdf" },
            ".hidden.glb": { "sha256": glb, "size": 3 },
            "b.glb": { "sha256": "nope", "size": 3 },
        }),
    );
    let (status, refused) = call(&app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let found = violations(&refused);
    let expect = [
        ("/attachments", "at most 2 attachments, not 4"),
        (
            "/attachments/.hidden.glb",
            "a name is 1 to 120 characters, with no / or \\, not starting with a dot",
        ),
        (
            "/attachments/a.glb/size",
            "the stored blob is 3 bytes, not 4",
        ),
        (
            "/attachments/b.glb/sha256",
            "must be 64 lowercase hex digits",
        ),
        (
            "/attachments/notes.pdf",
            "this plugin takes .glb, image/*, not application/pdf",
        ),
        (
            "/payload/files/1",
            "no attachment \"missing.glb\" on this review",
        ),
        (
            "/payload/files/2/$attachment",
            "must be the name of an attachment on this review",
        ),
    ];
    for (path, message) in expect {
        assert!(
            found.contains(&(path.to_string(), message.to_string())),
            "{path}: {message}\nin {found:?}"
        );
    }
    assert_eq!(found.len(), expect.len(), "{found:?}");
}

#[tokio::test]
async fn a_review_lists_its_files_and_serves_each_only_as_a_download() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    files_plugin(&app, scratch.path(), json!({ "accept": [".glb", ".html"] })).await;
    let model = b"glTF binary, more or less".to_vec();
    let page = b"<script>fetch('/api/v1/reviews')</script>".to_vec();
    let model_hash = upload(&app, &model).await;
    let page_hash = upload(&app, &page).await;
    let body = with_files(
        json!({ "files": [{ "$attachment": "Pivot lamp.glb" }, { "$attachment": "evil.html" }] }),
        json!({
            "Pivot lamp.glb": { "sha256": model_hash, "size": model.len(), "media_type": "model/gltf-binary" },
            "evil.html": { "sha256": page_hash, "size": page.len(), "media_type": "text/html" },
        }),
    );
    let (_, created) = call(&app, "POST", "/api/v1/reviews", Some(body)).await;
    let id = created["id"].as_str().unwrap().to_string();

    // the review says what it carries, the listing does not, as with the payload
    assert_eq!(
        created["attachments"],
        json!([
            { "name": "Pivot lamp.glb", "size": model.len(), "media_type": "model/gltf-binary", "sha256": model_hash },
            { "name": "evil.html", "size": page.len(), "media_type": "text/html", "sha256": page_hash },
        ])
    );
    let (_, shown) = call(&app, "GET", &format!("/api/v1/reviews/{id}"), None).await;
    assert_eq!(shown["attachments"], created["attachments"]);
    let (_, listing) = call(&app, "GET", "/api/v1/reviews", None).await;
    assert!(listing["reviews"][0].get("attachments").is_none());
    // but it does say how many, and how big, for the inbox row
    let total = json!({ "count": 2, "bytes": model.len() + page.len() });
    assert_eq!(listing["reviews"][0]["attachments_total"], total);
    assert_eq!(created["attachments_total"], total);
    let (_, plain) = call(&app, "POST", "/api/v1/reviews", Some(submission())).await;
    assert_eq!(plain["attachments"], json!([]));
    assert_eq!(
        plain["attachments_total"],
        json!({ "count": 0, "bytes": 0 })
    );

    let get = |name: &str| {
        let uri = format!(
            "/api/v1/reviews/{id}/attachments/{}",
            name.replace(' ', "%20")
        );
        let request = Request::get(uri)
            .header("host", "127.0.0.1:4747")
            .body(Body::empty())
            .unwrap();
        app.router.clone().oneshot(request)
    };
    let response = get("Pivot lamp.glb").await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers().clone();
    assert_eq!(headers["content-type"], "application/octet-stream");
    assert_eq!(headers["content-length"], model.len().to_string());
    assert_eq!(
        headers["content-disposition"],
        "attachment; filename*=UTF-8''Pivot%20lamp.glb"
    );
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(
        headers["content-security-policy"],
        "sandbox; default-src 'none'"
    );
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        model
    );

    // an uploaded page is a download like any other, never a page in this origin
    let response = get("evil.html").await.unwrap();
    assert_eq!(
        response.headers()["content-type"],
        "application/octet-stream"
    );
    assert_eq!(
        response.headers()["content-security-policy"],
        "sandbox; default-src 'none'"
    );
    assert!(
        response.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment;")
    );

    // only names this review carries: not another review's, not a path
    for name in ["nope.glb", "..%2F..%2Fpinrail.db", &model_hash] {
        let response = get(name).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{name}");
    }
    let other = plain["id"].as_str().unwrap();
    let request = Request::get(format!("/api/v1/reviews/{other}/attachments/evil.html"))
        .header("host", "127.0.0.1:4747")
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(
        body["message"],
        format!("review {other} carries no attachment \"evil.html\"")
    );
}

#[tokio::test]
async fn text_is_never_taken_for_a_reference() {
    let app = app();
    // a list of release checks: CI calls its build outputs attachments
    let mut body = submission();
    body["payload"] = json!({ "groups": [{ "title": "Release 2.4", "items": [
        { "id": 1, "title": "artifact:linux-x64 was not uploaded by the release job" },
    ] }] });
    let (status, created) = call(&app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
}

#[tokio::test]
async fn a_plugin_whose_attachments_block_is_broken_is_not_installed() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    let cases = [
        (
            json!({ "accept": ["glb"] }),
            "attachments.accept: \"glb\" is neither an extension like .glb nor a media type like image/png",
        ),
        (json!({ "accepts": [".glb"] }), "attachments"),
        (
            json!({ "accept": [".glb"], "max_count": 99 }),
            "attachments/max_count",
        ),
        (json!({ "accept": [] }), "attachments/accept"),
    ];
    for (i, (block, said)) in cases.into_iter().enumerate() {
        let dir = scratch.path().join(format!("files{i}"));
        view(&dir, "<html></html>");
        schemas(&dir, json!({}));
        std::fs::write(
            dir.join("manifest.json"),
            json!({ "name": "files", "version": "1.0.0", "attachments": block }).to_string(),
        )
        .unwrap();
        let (status, refused) = install(&app, &dir, json!({ "link": true })).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{block}");
        assert!(
            refused["message"].as_str().unwrap().contains(said),
            "{block}: {refused}"
        );
    }
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

/// An installed bundle: the official list plugin copied under another name
/// and installed from its folder. A file changed behind the app's back is
/// flagged on the plugin's row and not served.
#[cfg(unix)]
#[tokio::test]
async fn an_installed_bundle_is_served_and_a_tampered_one_is_flagged() {
    use std::os::unix::fs::PermissionsExt;
    let app = app();
    let source = tempfile::tempdir().unwrap();
    let list = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/list");
    copy_tree(&list, source.path());
    let manifest_path = source.path().join("manifest.json");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest["name"] = json!("shelf");
    std::fs::write(manifest_path, manifest.to_string()).unwrap();

    let (status, installed) = install(&app, source.path(), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{installed}");
    assert_eq!(installed["name"], "shelf");
    assert_eq!(installed["install"]["link"], false);
    assert_eq!(installed["install"]["source_kind"], "folder");
    let bundle = installed["install"]["bundle"].as_str().unwrap().to_string();
    assert_eq!(
        bundle,
        Listing::of_folder(source.path(), Taken::FromSource)
            .unwrap()
            .hash()
    );
    let stored = app.state.bundles().path(&bundle);
    assert_eq!(installed["path"], stored.display().to_string());
    assert_eq!(installed["install"]["previous"], Value::Null);
    assert_eq!(installed["install"]["modified"], false);

    // a review renders with the bundle
    let mut body = submission();
    body["plugin"] = json!("shelf");
    let review = submit(&app, body).await;
    assert_eq!(review["plugin"], "shelf", "{review}");
    let id = review["id"].as_str().unwrap();
    let (_, view) = call(&app, "GET", &format!("/api/v1/reviews/{id}/view"), None).await;
    let url = view["url"].as_str().unwrap().to_string();
    assert_eq!(url, format!("/bundles/{bundle}/view/index.html"));
    assert_eq!(bundle_get(&app, &url, &[]).await.status(), StatusCode::OK);

    // changed behind the app's back
    let page = stored.join("view/index.html");
    std::fs::set_permissions(&page, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::write(&page, "<html>changed</html>").unwrap();
    call(&app, "POST", "/api/v1/plugins/reload", None).await;
    let (_, body) = call(&app, "GET", "/api/v1/plugins", None).await;
    let shelf = body["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "shelf")
        .unwrap();
    assert_eq!(shelf["install"]["modified"], true);
    assert_eq!(
        bundle_get(&app, &url, &[]).await.status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

/// A copy of a sample plugin with its manifest's version rewritten.
/// Copies a plugin folder as a bundle: without its tests and fixtures.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for file in std::fs::read_dir(from).unwrap().flatten() {
        let name = file.file_name();
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

/// Every file under `dir`, by its path relative to it with `/` between
/// the parts, sorted.
fn files_under(dir: &std::path::Path) -> Vec<String> {
    fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path.strip_prefix(root).unwrap();
                out.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn files_under_listing(listing: &Listing) -> Vec<String> {
    listing.files.iter().map(|f| f.path.clone()).collect()
}

fn plugin_copy(root: &std::path::Path, name: &str, version: &str) -> std::path::PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins")
        .join(name);
    let to = root.join(format!("{name}-{}", version.replace('.', "_")));
    copy_tree(&from, &to);
    let manifest = std::fs::read_to_string(to.join("manifest.json")).unwrap();
    let mut manifest: Value = serde_json::from_str(&manifest).unwrap();
    manifest["version"] = json!(version);
    std::fs::write(to.join("manifest.json"), manifest.to_string()).unwrap();
    to
}

/// Installs as a person does, after looking at the source: the plugin's
/// row with 200, or the refusal with 422.
async fn install(app: &App, source: &std::path::Path, extra: Value) -> (StatusCode, Value) {
    let mut body = json!({ "source": source.display().to_string() });
    if let Value::Object(map) = extra {
        for (k, v) in map {
            body[k] = v;
        }
    }
    call(app, "POST", "/api/v1/plugins/inspect", Some(body.clone())).await;
    call(app, "POST", "/api/v1/plugins/install", Some(body)).await
}

/// The bundle a review's frame loads.
async fn view_url(app: &App, review: &Value) -> String {
    let id = review["id"].as_str().unwrap();
    let (status, view) = call(app, "GET", &format!("/api/v1/reviews/{id}/view"), None).await;
    assert_eq!(status, StatusCode::OK, "{view}");
    view["url"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn installing_from_a_folder_stores_a_bundle_new_reviews_use() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();

    // the sample as it is: 1.0.0, stored as a bundle; with a
    // secret and a dependency folder beside it, which the bundle leaves out
    let hello = plugin_copy(scratch.path(), "hello", "1.0.0");
    std::fs::write(hello.join(".env"), "TOKEN=secret").unwrap();
    std::fs::create_dir_all(hello.join("node_modules/x")).unwrap();
    std::fs::write(hello.join("node_modules/x/index.js"), "").unwrap();
    // and what the layout does not name: notes, a cache, a hidden file
    std::fs::write(hello.join("notes.txt"), "to do").unwrap();
    std::fs::create_dir_all(hello.join("cache")).unwrap();
    std::fs::write(hello.join("cache/build.bin"), "x").unwrap();
    std::fs::write(hello.join("view/.DS_Store"), "x").unwrap();
    let bundle = Listing::of_folder(&hello, Taken::FromSource).unwrap();
    let (status, row) = install(&app, &hello, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["name"], "hello");
    assert_eq!(row["name"], "hello");
    assert_eq!(row["version"], "1.0.0");
    assert_eq!(row["install"]["source_kind"], "folder");
    assert_eq!(row["install"]["link"], false);
    assert_eq!(row["install"]["bundle"], bundle.hash());
    assert_eq!(row["replaced_version"], Value::Null, "{row}");
    // a plugin is its name; where it came from is its installation's
    assert!(
        row.get("plugin").is_none() && row.get("publisher").is_none(),
        "{row}"
    );
    let mut keys: Vec<&str> = row["install"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "bundle",
            "installed_at",
            "link",
            "modified",
            "source",
            "source_kind",
            "updated_at"
        ]
    );
    assert_eq!(row["install"]["source"], hello.display().to_string());
    // the store holds the bundle: its files, and nothing else
    let stored = app.state.bundles().path(&bundle.hash());
    assert_eq!(files_under(&stored), files_under_listing(&bundle));
    assert_eq!(row["path"], stored.display().to_string());

    // a review records the exact version and the bundle it renders with
    let mut body = submission();
    body["plugin"] = json!("hello");
    body["payload"] = json!({"message": "hi"});
    let review = submit(&app, body.clone()).await;
    assert_eq!(review["plugin"], "hello");
    assert_eq!(review["plugin_version"], "1.0.0");
    assert_eq!(review["plugin_bundle"], bundle.hash());
    assert_eq!(
        view_url(&app, &review).await,
        format!("/bundles/{}/view/index.html", bundle.hash())
    );

    // a patch is what new reviews use; the review keeps its release
    let patch = plugin_copy(scratch.path(), "hello", "1.0.4");
    std::fs::write(patch.join("view/index.html"), "<html>1.0.4</html>").unwrap();
    let patched = Listing::of_folder(&patch, Taken::FromSource)
        .unwrap()
        .hash();
    let (status, row) = install(&app, &patch, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["version"], "1.0.4");
    assert_eq!(row["install"]["bundle"], patched);
    assert_eq!(
        view_url(&app, &review).await,
        format!("/bundles/{}/view/index.html", bundle.hash())
    );
    let (_, shown) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews/{}", review["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(
        shown["plugin_version"], "1.0.0",
        "the review keeps what it was submitted to"
    );

    // an older one replaces it too, and the answer says it is older
    assert_eq!(row["replaced_version"], "1.0.0", "{row}");
    assert_eq!(row["older"], false, "{row}");
    let older = plugin_copy(scratch.path(), "hello", "1.0.2");
    let (status, row) = install(&app, &older, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["version"], "1.0.2");
    assert_eq!(row["replaced_version"], "1.0.4", "{row}");
    assert_eq!(row["older"], true, "{row}");

    // a new major whose decision schema has another shape
    let next = plugin_copy(scratch.path(), "hello", "2.0.0");
    std::fs::write(
        next.join("schemas/decision.schema.json"),
        json!({"type": "object", "required": ["answer"], "properties": {"answer": {"type": "string"}}})
            .to_string(),
    )
    .unwrap();
    let (status, row) = install(&app, &next, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["version"], "2.0.0");
    assert_eq!(row["replaced_version"], "1.0.2", "{row}");
    assert_eq!(row["older"], false, "{row}");
    let url = view_url(&app, &review).await;
    assert_eq!(bundle_get(&app, &url, &[]).await.status(), StatusCode::OK);

    // each review's decision is checked against its own release's schema
    body["title"] = json!("with 2.0.0");
    let newer = submit(&app, body).await;
    assert_eq!(newer["plugin_version"], "2.0.0");
    let decide = |review: &Value, data: Value| {
        let id = review["id"].as_str().unwrap().to_string();
        let app = &app;
        async move {
            call(
                app,
                "POST",
                &format!("/api/v1/reviews/{id}/decision"),
                Some(json!({ "data": data })),
            )
            .await
            .0
        }
    };
    assert_eq!(
        decide(&newer, json!({"ok": true})).await,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(decide(&review, json!({"ok": true})).await, StatusCode::OK);
    assert_eq!(
        decide(&newer, json!({"answer": "yes"})).await,
        StatusCode::OK
    );

    // a link serves the folder live, and a folder with no manifest is refused
    let (status, row) = install(&app, &hello, json!({"link": true})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["install"]["link"], true);
    assert_eq!(row["name"], "hello");
    assert_eq!(
        row["path"],
        std::path::absolute(&hello).unwrap().display().to_string()
    );
    let (status, body) = install(&app, scratch.path(), json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("/source: not a plugin: cannot read manifest.json"),
        "{body}"
    );
}

/// A zip of every file under `dir`, at the archive's root or inside a
/// folder named `top`, the way `zip -r` makes one.
fn zip_of_folder(dir: &std::path::Path, top: Option<&str>) -> Vec<u8> {
    use std::io::Write;
    fn add(
        out: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>,
        root: &std::path::Path,
        dir: &std::path::Path,
        top: Option<&str>,
    ) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                add(out, root, &path, top);
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let name = match top {
                Some(top) => format!("{top}/{relative}"),
                None => relative,
            };
            out.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            out.write_all(&std::fs::read(&path).unwrap()).unwrap();
        }
    }
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    add(&mut out, dir, dir, top);
    out.finish().unwrap().into_inner()
}

#[tokio::test]
async fn a_zip_on_disk_installs_the_bundle_its_folder_would() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    // a built plugin with its sources and a hidden file beside the view
    let hello = plugin_copy(scratch.path(), "hello", "1.2.0");
    std::fs::create_dir_all(hello.join("src")).unwrap();
    std::fs::write(hello.join("src/main.ts"), "export {}").unwrap();
    std::fs::write(hello.join(".env"), "TOKEN=secret").unwrap();
    let bundle = Listing::of_folder(&hello, Taken::FromSource)
        .unwrap()
        .hash();

    // the files at the archive's root, or inside one folder at its root
    for (file, top) in [("hello-1.2.0.zip", None), ("hello.zip", Some("hello"))] {
        let zip = scratch.path().join(file);
        std::fs::write(&zip, zip_of_folder(&hello, top)).unwrap();
        let (status, row) = install(&app, &zip, json!({})).await;
        assert_eq!(status, StatusCode::OK, "{file}: {row}");
        assert_eq!(row["name"], "hello");
        assert_eq!(row["version"], "1.2.0");
        assert_eq!(row["install"]["source_kind"], "archive", "{file}");
        assert_eq!(row["install"]["source"], zip.display().to_string());
        assert_eq!(
            row["install"]["bundle"], bundle,
            "{file}: the folder's bundle"
        );
    }

    // an entry that would leave the archive is refused, and the plugin
    // stays as it was installed
    let evil = scratch.path().join("evil.zip");
    std::fs::write(
        &evil,
        zipped(&[
            (
                "manifest.json",
                &json!({"name": "hello", "version": "9.0.0"}).to_string(),
            ),
            ("../outside.txt", "x"),
        ]),
    )
    .unwrap();
    let (status, body) = install(&app, &evil, json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("leaves the archive"),
        "{body}"
    );
    assert!(!scratch.path().join("outside.txt").exists());
    let (_, listed) = call(&app, "GET", "/api/v1/plugins", None).await;
    let hello_row = listed["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "hello")
        .unwrap();
    assert_eq!(hello_row["version"], "1.2.0");

    // a file that is not a zip, and a link to a zip, are refused
    let not_zip = scratch.path().join("notes.zip");
    std::fs::write(&not_zip, "not a zip").unwrap();
    let (status, body) = install(&app, &not_zip, json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("not a zip archive"),
        "{body}"
    );
    let zip = scratch.path().join("hello.zip");
    let (status, body) = install(&app, &zip, json!({"link": true})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("a link needs a folder")
            || body["message"]
                .as_str()
                .unwrap_or_default()
                .contains("a link needs a folder"),
        "{body}"
    );
}

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
    assert_eq!(seen["source_kind"], "folder");
    assert_eq!(seen["source"], plain.display().to_string());
    assert_eq!(seen["installed"], Value::Null);
    assert_eq!(seen["link"], false);
    assert!(
        db(&app).install("hello").unwrap().is_none(),
        "inspecting installed something"
    );

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

    // the folder is compared with the installed bundle: what a bundle
    // leaves out does not count as a change
    std::fs::create_dir_all(plain.join("tests")).unwrap();
    std::fs::write(plain.join("tests/plain.spec.ts"), "test").unwrap();
    std::fs::write(plain.join(".editorconfig"), "root = true").unwrap();
    std::fs::write(plain.join("notes.txt"), "to do").unwrap();
    std::fs::write(plain.join("view/.DS_Store"), "x").unwrap();
    let (_, seen) = call(
        &app,
        "POST",
        "/api/v1/plugins/inspect",
        Some(json!({"source": plain.display().to_string()})),
    )
    .await;
    assert_eq!(seen["installed"]["unchanged"], true, "{seen}");
    std::fs::write(plain.join("view/index.html"), "<html>changed</html>").unwrap();
    let (_, seen) = call(
        &app,
        "POST",
        "/api/v1/plugins/inspect",
        Some(json!({"source": plain.display().to_string()})),
    )
    .await;
    assert_eq!(seen["installed"]["unchanged"], false, "{seen}");
    let older = plugin_copy(&scratch.path().join("older"), "hello", "1.0.0");
    let (status, seen) = call(
        &app,
        "POST",
        "/api/v1/plugins/inspect",
        Some(json!({"source": older.display().to_string()})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{seen}");
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
async fn removing_drops_the_installation_and_reviews_keep_their_bundles() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    let source = plugin_copy(scratch.path(), "hello", "1.0.0");
    let (status, row) = install(&app, &source, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    let bundle = row["install"]["bundle"].as_str().unwrap().to_string();

    // a plugin the app ships cannot be removed
    let (status, body) = call(&app, "DELETE", "/api/v1/plugins/list", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("ships with Pinrail"),
        "{body}"
    );

    // no review was made with it: the bundle goes with the next sweep
    let (status, answer) = call(&app, "DELETE", "/api/v1/plugins/hello", None).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["removed"], "hello");
    assert!(db(&app).install("hello").unwrap().is_none());
    let later = chrono::Utc::now() + chrono::Duration::minutes(1);
    assert_eq!(app.state.bundles().sweep(later).unwrap(), 1);
    assert!(!app.state.bundles().path(&bundle).exists());
    let (_, listed) = call(&app, "GET", "/api/v1/plugins", None).await;
    assert!(
        listed["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["name"] != "hello"),
        "{listed}"
    );

    // a review keeps the bundle it was submitted to
    let (status, row) = install(&app, &source, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    let mut body = submission();
    body["plugin"] = json!("hello");
    body["payload"] = json!({"message": "keep me"});
    let review = submit(&app, body).await;
    let (status, answer) = call(&app, "DELETE", "/api/v1/plugins/hello", None).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(app.state.bundles().sweep(later).unwrap(), 0);
    let url = view_url(&app, &review).await;
    assert_eq!(bundle_get(&app, &url, &[]).await.status(), StatusCode::OK);
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
    assert_eq!(answer["link"], true);
    assert!(source.join("manifest.json").is_file());
    let (status, _) = call(&app, "DELETE", "/api/v1/plugins/hello", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// The page a review's frame shows: its view address, fetched.
async fn served(app: &App, review: &Value) -> String {
    let url = view_url(app, review).await;
    let response = bundle_get(app, &url, &[]).await;
    assert_eq!(response.status(), StatusCode::OK, "{url}");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A review shows the release it was submitted to: an update is for new
/// reviews. A linked plugin's reviews show the folder live while it is
/// linked, and the folder as it was at submit once the link is gone.
#[tokio::test]
async fn a_review_shows_its_own_release_and_a_link_is_live() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    let first = plugin_copy(scratch.path(), "hello", "1.0.0");
    std::fs::write(first.join("view/index.html"), "<html>first</html>").unwrap();
    let (status, row) = install(&app, &first, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    let mut body = submission();
    body["plugin"] = json!("hello");
    body["payload"] = json!({"message": "hi"});
    let review = submit(&app, body.clone()).await;
    assert_eq!(served(&app, &review).await, "<html>first</html>");

    // a patch is for new reviews
    let second = plugin_copy(scratch.path(), "hello", "1.0.1");
    std::fs::write(second.join("view/index.html"), "<html>second</html>").unwrap();
    let (status, row) = install(&app, &second, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(served(&app, &review).await, "<html>first</html>");
    body["payload"] = json!({"message": "after the patch"});
    let patched = submit(&app, body.clone()).await;
    assert_eq!(served(&app, &patched).await, "<html>second</html>");

    // a link is served live, to every review of the plugin, and each review
    // made with it records the folder as it was
    let live = plugin_copy(scratch.path(), "hello", "1.1.0");
    std::fs::write(live.join("view/index.html"), "<html>live</html>").unwrap();
    let (status, _) = install(&app, &live, json!({"link": true})).await;
    assert_eq!(status, StatusCode::OK);
    body["payload"] = json!({"message": "while linked"});
    let linked = submit(&app, body).await;
    assert!(linked["plugin_bundle"].is_string());
    std::fs::write(live.join("view/index.html"), "<html>edited</html>").unwrap();
    assert_eq!(served(&app, &linked).await, "<html>edited</html>");
    assert_eq!(served(&app, &review).await, "<html>edited</html>");
    assert!(
        view_url(&app, &linked)
            .await
            .starts_with("/links/hello/view/")
    );

    // the link removed: each review shows the release it was made with
    let (status, _) = call(&app, "DELETE", "/api/v1/plugins/hello", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(served(&app, &review).await, "<html>first</html>");
    assert_eq!(served(&app, &patched).await, "<html>second</html>");
    assert_eq!(served(&app, &linked).await, "<html>live</html>");
}

#[tokio::test]
async fn start_tidies_the_plugins_folder() {
    // a zip unpacked in work by a crash, and the build logs of an earlier version
    let dir = tempfile::tempdir().unwrap();
    let plugins = dir.path().join("plugins");
    std::fs::create_dir_all(plugins.join("work/archive-abc")).unwrap();
    std::fs::write(plugins.join("work/archive-abc/file"), "x").unwrap();
    std::fs::create_dir_all(plugins.join("logs")).unwrap();
    std::fs::write(plugins.join("logs/old.log"), "x").unwrap();
    let mut config = Config::new(dir.path(), 0);
    config.user = "tester".into();
    let _state = Arc::new(Pinrail::open(config).unwrap());
    assert!(
        plugins.join("work").is_dir()
            && std::fs::read_dir(plugins.join("work"))
                .unwrap()
                .next()
                .is_none(),
        "work is emptied"
    );
    assert!(!plugins.join("logs").exists(), "the build logs are removed");
}

/// The two schemas every plugin has, in their places: the payload's as
/// given, the decision's taking anything.
fn schemas(dir: &std::path::Path, payload: Value) {
    std::fs::create_dir_all(dir.join("schemas")).unwrap();
    std::fs::write(dir.join("schemas/payload.schema.json"), payload.to_string()).unwrap();
    std::fs::write(dir.join("schemas/decision.schema.json"), "{}").unwrap();
}

/// A plugin's view in its place.
fn view(dir: &std::path::Path, html: &str) {
    std::fs::create_dir_all(dir.join("view")).unwrap();
    std::fs::write(dir.join("view/index.html"), html).unwrap();
}

/// A plugin's sources: a manifest, its schemas and the files its view is
/// built from, with no built view.
fn plugin_sources(root: &std::path::Path, name: &str) -> std::path::PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/view.txt"), "sources").unwrap();
    std::fs::write(dir.join("package.json"), "{}").unwrap();
    schemas(&dir, json!({}));
    std::fs::write(
        dir.join("manifest.json"),
        json!({"name": name, "version": "1.0.0"}).to_string(),
    )
    .unwrap();
    dir
}

/// A plugin's sources with no built view: installing says to build it
/// first and runs nothing, not even the build an earlier manifest
/// declared, and once its view is built the same folder links and
/// installs.
#[tokio::test]
async fn a_folder_that_is_not_built_is_refused_and_nothing_runs() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    let marker = scratch.path().join("ran");
    let sources = plugin_sources(scratch.path(), "built");
    std::fs::write(
        sources.join("manifest.json"),
        json!({"name": "built", "version": "1.0.0", "build": {"command": format!("touch {}", marker.display())}})
            .to_string(),
    )
    .unwrap();
    for link in [false, true] {
        let (status, body) = install(&app, &sources, json!({ "link": link })).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        let said = body["message"].as_str().unwrap();
        assert!(
            said.contains("view/index.html not found; build the plugin first"),
            "link {link}: {body}"
        );
    }
    assert!(!marker.exists(), "the build ran");

    std::fs::create_dir_all(sources.join("view")).unwrap();
    std::fs::write(sources.join("view/index.html"), "<html>built</html>").unwrap();
    let (status, row) = install(&app, &sources, json!({"link": true})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    let (status, row) = install(&app, &sources, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    // the sources beside the built view are left behind
    let stored = app
        .state
        .bundles()
        .path(row["install"]["bundle"].as_str().unwrap());
    assert!(stored.join("view/index.html").is_file());
    assert!(!stored.join("src").exists());
    assert!(!stored.join("package.json").exists());
    assert!(!marker.exists(), "the build ran");
}

/// A name that is not a plugin name is refused before anything is stored.
#[tokio::test]
async fn a_manifest_name_that_is_not_a_name_is_refused() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    let dir = plugin_sources(scratch.path(), "escaping");
    view(&dir, "<html>hi</html>");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    manifest["name"] = json!("../../escaped");
    std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
    let (status, job) = install(&app, &dir, json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{job}");
    assert!(job["message"].as_str().unwrap().contains("name"), "{job}");
}

/// A bundle is its files, wherever they came from: the same plugin from a
/// folder and from a repository on this machine is one plugin with one
/// bundle, and installing it again after removing it makes the kept line
/// current again.
#[tokio::test]
async fn the_same_files_from_two_sources_are_one_bundle() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    let folder = plugin_copy(&scratch.path().join("folder"), "hello", "1.0.0");
    let (status, from_folder) = install(&app, &folder, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{from_folder}");
    let zip = scratch.path().join("hello.zip");
    std::fs::write(&zip, zip_of_folder(&folder, Some("hello"))).unwrap();
    let (status, from_zip) = install(&app, &zip, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{from_zip}");

    // one plugin, and one bundle
    assert_eq!(from_folder["name"], "hello");
    assert_eq!(from_zip["name"], "hello");
    assert_eq!(from_zip["install"]["source_kind"], "archive");
    let bundle = from_folder["install"]["bundle"].as_str().unwrap();
    assert_eq!(from_zip["install"]["bundle"], bundle);
    // the zip replaced the folder's installation under the one name
    assert_eq!(from_zip["replaced_version"], "1.0.0");
    let (_, listed) = call(&app, "GET", "/api/v1/plugins", None).await;
    let hellos: Vec<&Value> = listed["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"] == "hello")
        .collect();
    assert_eq!(hellos.len(), 1);
    assert_eq!(hellos[0]["install"]["source_kind"], "archive");
    let stored: Vec<_> = std::fs::read_dir(app.state.config().plugin_bundles_dir())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name() == bundle)
        .collect();
    assert_eq!(stored.len(), 1);

    // removed while a review uses it, then installed again: the same bundle
    let mut body = submission();
    body["plugin"] = json!("hello");
    body["payload"] = json!({"message": "hi"});
    let review = submit(&app, body).await;
    let (status, _) = call(&app, "DELETE", "/api/v1/plugins/hello", None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, again) = install(&app, &folder, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["install"]["bundle"], bundle);
    assert_eq!(
        view_url(&app, &review).await,
        format!("/bundles/{bundle}/view/index.html")
    );
}

/// An update is installed whatever it changes: the reviews made before it
/// keep the release they were submitted to, and decide by its schema,
/// while new reviews use the new one.
#[tokio::test]
async fn an_update_that_changes_the_schemas_leaves_earlier_reviews_as_they_were() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    let first = plugin_copy(&scratch.path().join("a"), "hello", "1.0.0");
    let (status, row) = install(&app, &first, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    let mut body = submission();
    body["plugin"] = json!("hello");
    body["payload"] = json!({"message": "before"});
    let before = submit(&app, body.clone()).await;

    // the comment now required, without a new major
    let breaking = plugin_copy(&scratch.path().join("b"), "hello", "1.1.0");
    let path = breaking.join("schemas/decision.schema.json");
    let mut decision: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    decision["required"] = json!(["ok", "comment"]);
    std::fs::write(&path, decision.to_string()).unwrap();
    let (status, row) = install(&app, &breaking, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    body["payload"] = json!({"message": "after"});
    let after = submit(&app, body).await;
    assert_eq!(before["plugin_version"], "1.0.0");
    assert_eq!(after["plugin_version"], "1.1.0");
    assert_ne!(before["plugin_bundle"], after["plugin_bundle"]);

    let decide = |review: &Value, data: Value| {
        let id = review["id"].as_str().unwrap().to_string();
        let app = &app;
        async move {
            call(
                app,
                "POST",
                &format!("/api/v1/reviews/{id}/decision"),
                Some(json!({ "data": data })),
            )
            .await
            .0
        }
    };
    assert_eq!(
        decide(&after, json!({"ok": true})).await,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(decide(&before, json!({"ok": true})).await, StatusCode::OK);
    assert_eq!(
        decide(&after, json!({"ok": true, "comment": "fine"})).await,
        StatusCode::OK
    );
}

/// Working on a plugin the app carries: a link under its name takes its
/// place, so its reviews render with the folder, and removing the link
/// removes the installation, while the app's bundle stays for the reviews
/// made with it.
#[tokio::test]
async fn a_link_takes_the_place_of_the_apps_own_plugin_until_it_is_removed() {
    let app = app();
    let scratch = tempfile::tempdir().unwrap();
    let published = current_bundle(&app, "list").unwrap();
    let review = submit(&app, submission()).await;
    assert_eq!(review["plugin"], "list");

    let mine = plugin_copy(scratch.path(), "list", "1.0.0");
    std::fs::write(mine.join("view/index.html"), "<html>my fix</html>").unwrap();
    let (status, seen) = call(
        &app,
        "POST",
        "/api/v1/plugins/inspect",
        Some(json!({"source": mine.display().to_string(), "link": true})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{seen}");
    assert_eq!(seen["installed"]["source_kind"], "app");
    assert_eq!(seen["installed"]["links_kept"], false);

    let (status, row) = install(&app, &mine, json!({"link": true})).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["name"], "list");
    assert_eq!(row["install"]["link"], true);
    assert_eq!(row["install"]["source_kind"], "folder");
    // one plugin of the name, and the reviews render with the folder
    let (_, listed) = call(&app, "GET", "/api/v1/plugins", None).await;
    let lists = listed["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"] == "list")
        .count();
    assert_eq!(lists, 1);
    assert_eq!(served(&app, &review).await, "<html>my fix</html>");

    let (status, answer) = call(&app, "DELETE", "/api/v1/plugins/list", None).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["link"], true);
    assert_eq!(current_bundle(&app, "list"), None);
    // the review keeps the bundle it was submitted to
    let later = chrono::Utc::now() + chrono::Duration::minutes(1);
    app.state.bundles().sweep(later).unwrap();
    assert!(app.state.bundles().path(&published).is_dir());
    assert_ne!(served(&app, &review).await, "<html>my fix</html>");
}

/// A zip of `files` as `path → content`, the way a plugin's zip carries
/// its bundle.
fn zipped(files: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let plain = zip::write::SimpleFileOptions::default();
    for (path, content) in files {
        out.start_file(*path, plain).unwrap();
        out.write_all(content.as_bytes()).unwrap();
        // every bundle has its two schemas beside its manifest
        if let Some(root) = path.strip_suffix("manifest.json") {
            for schema in [
                "schemas/payload.schema.json",
                "schemas/decision.schema.json",
            ] {
                out.start_file(format!("{root}{schema}"), plain).unwrap();
                out.write_all(b"{}").unwrap();
            }
        }
    }
    out.finish().unwrap().into_inner()
}

#[tokio::test]
async fn plugins_describe_themselves_and_a_submission_validates_without_being_stored() {
    let app = app();
    let (status, body) = call(&app, "GET", "/api/v1/plugins/describe", None).await;
    assert_eq!(status, StatusCode::OK);
    let plugins = body["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 2, "the built-in ones: {body}");

    let (status, body) = call(&app, "GET", "/api/v1/plugins/list/describe", None).await;
    assert_eq!(status, StatusCode::OK);
    let list = &body["plugins"][0];
    assert_eq!(list["name"], "list");
    assert!(
        list["payload_schema"]["$ref"].is_null()
            && list["payload_schema"]["properties"].is_object(),
        "the $ref is read in: {list}"
    );
    assert!(list["decision_schema"]["properties"].is_object());
    assert!(list["use_when"].is_string() && list["example"]["groups"].is_array());
    let (status, _) = call(&app, "GET", "/api/v1/plugins/nope/describe", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, body) = call(&app, "POST", "/api/v1/reviews/validate", Some(submission())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["valid"], true);
    assert_eq!(body["plugin"], "list");
    assert_eq!(body["plugin_version"], "1.0.0");
    let (_, listing) = call(&app, "GET", "/api/v1/reviews", None).await;
    assert_eq!(listing["total"], 0, "nothing was stored");

    let mut bad = submission();
    bad["payload"]["groups"] = json!("nope");
    let (status, validated) =
        call(&app, "POST", "/api/v1/reviews/validate", Some(bad.clone())).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (_, submitted) = call(&app, "POST", "/api/v1/reviews", Some(bad)).await;
    assert_eq!(
        violations(&validated),
        violations(&submitted),
        "the same checks as a submission"
    );
}

#[tokio::test]
async fn a_plugin_sample_is_sent_over_http_and_listed_as_there() {
    let app = app();
    let (status, body) = call(&app, "POST", "/api/v1/plugins/list/sample", None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["status"], "pending");
    assert_eq!(body["requested_by"], "sample");
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/plugins/list/sample",
        Some(json!({ "title": "Mine" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["title"], "Mine");
    let (status, body) = call(&app, "POST", "/api/v1/plugins/nope/sample", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        violations(&body),
        [(
            "/plugin".to_string(),
            "no usable plugin is named nope".to_string()
        )]
    );

    let (_, listed) = call(&app, "GET", "/api/v1/plugins", None).await;
    let list = listed["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "list")
        .unwrap();
    assert_eq!(list["samples"], json!(["list"]));
    let (_, described) = call(&app, "GET", "/api/v1/plugins/list/describe", None).await;
    assert_eq!(described["plugins"][0]["samples"], json!(["list"]));

    // a sample sent by name, and a name the plugin has no sample of
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/plugins/list/sample",
        Some(json!({ "sample": "list" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/plugins/list/sample",
        Some(json!({ "sample": "nope" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        violations(&body),
        [(
            "/sample".to_string(),
            "list has no sample named nope; its samples are list".to_string()
        )]
    );
}

#[tokio::test]
async fn a_review_has_a_preview_page_for_a_browser() {
    let app = app();
    let request = Request::builder()
        .uri("/preview/reviews/r_anything")
        .header("host", "127.0.0.1:4747")
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()[axum::http::header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert!(
        response.headers()[axum::http::header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .contains("frame-src 'self'")
    );
    // no other site may frame it
    assert!(
        response.headers()[axum::http::header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .contains("frame-ancestors 'none'")
    );
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(String::from_utf8_lossy(&body).contains(r#"sandbox="allow-scripts""#));
}

/// Every answer says its content type is final, so a browser never reads
/// JSON or markdown as a page or a script: answers, refusals and pages alike.
#[tokio::test]
async fn no_answer_leaves_its_content_type_to_be_sniffed() {
    let app = app();
    for uri in [
        "/api/v1/info",
        "/api/v1/reviews/r_missing",
        "/preview/reviews/r_anything",
    ] {
        let request = Request::builder()
            .uri(uri)
            .header("host", "127.0.0.1:4747")
            .body(Body::empty())
            .unwrap();
        let response = app.router.clone().oneshot(request).await.unwrap();
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::X_CONTENT_TYPE_OPTIONS)
                .map(|v| v.to_str().unwrap()),
            Some("nosniff"),
            "{uri}"
        );
    }
}

#[tokio::test]
async fn a_decision_is_checked_without_deciding_on_a_dry_run() {
    let app = app();
    let review = submit(&app, submission()).await;
    let id = review["id"].as_str().unwrap();
    let path = format!("/api/v1/reviews/{id}/decision?dry_run=true");

    let good = json!({ "decisions": [{ "id": 1, "action": "accept" }], "undecided": [] });
    let (status, body) = call(&app, "POST", &path, Some(json!({ "data": good }))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "valid": true, "data": good }));

    let (status, body) = call(&app, "POST", &path, Some(json!({ "data": { "nope": 1 } }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(!violations(&body).is_empty());

    let (_, after) = call(&app, "GET", &format!("/api/v1/reviews/{id}"), None).await;
    assert_eq!(after["status"], "pending", "a dry run decides nothing");
}

#[test]
fn a_port_in_use_is_refused_at_bind_with_its_address() {
    // taken before anything else starts, so the app can say why it cannot
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = held.local_addr().unwrap().port();
    let dir = tempfile::tempdir().unwrap();
    let error = pinrail_core::api::bind(&Config::new(dir.path(), port)).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
    assert!(
        error.to_string().contains(&format!("127.0.0.1:{port}")),
        "{error}"
    );

    // a free port binds: the operating system's pick, since another test
    // may take the one just held as soon as it is let go
    assert!(pinrail_core::api::bind(&Config::new(dir.path(), 0)).is_ok());
    drop(held);
}
