//! Stars the person gives a round of a review, so they can find the review
//! again: a review is starred when any of its rounds is.

use axum::http::StatusCode;
use serde_json::{Value, json};

mod common;
use common::{App, app, call};

fn review(title: &str) -> Value {
    json!({
        "plugin": "list",
        "title": title,
        "origin": {"repo": "acme"},
        "payload": {"groups": [{"title": "g", "items": [{"id": 1, "title": "x"}]}]}
    })
}

async fn send(app: &App, body: Value) -> String {
    let (status, sent) = call(app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{sent}");
    sent["id"].as_str().unwrap().to_string()
}

async fn star(app: &App, method: &str, id: &str) -> StatusCode {
    call(app, method, &format!("/api/v1/reviews/{id}/star"), None)
        .await
        .0
}

async fn get(app: &App, id: &str) -> Value {
    call(app, "GET", &format!("/api/v1/reviews/{id}"), None)
        .await
        .1
}

async fn starred_ids(app: &App, query: &str) -> Vec<String> {
    let (status, listed) = call(
        app,
        "GET",
        &format!("/api/v1/reviews?starred=true{query}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    listed["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn a_star_is_given_and_removed_and_twice_changes_nothing() {
    let app = app();
    let id = send(&app, review("Keep this plan")).await;
    let other = send(&app, review("Not this one")).await;
    assert_eq!(get(&app, &id).await["starred"], false);
    assert_eq!(get(&app, &id).await["starred_at"], Value::Null);

    assert_eq!(star(&app, "PUT", &id).await, StatusCode::NO_CONTENT);
    let first = get(&app, &id).await["starred_at"].clone();
    assert!(first.is_string(), "{first}");
    assert_eq!(get(&app, &id).await["starred"], true);
    // starred again, it keeps its first time
    assert_eq!(star(&app, "PUT", &id).await, StatusCode::NO_CONTENT);
    assert_eq!(get(&app, &id).await["starred_at"], first);
    assert_eq!(starred_ids(&app, "").await, std::slice::from_ref(&id));
    assert_eq!(get(&app, &other).await["starred"], false);

    assert_eq!(star(&app, "DELETE", &id).await, StatusCode::NO_CONTENT);
    assert_eq!(star(&app, "DELETE", &id).await, StatusCode::NO_CONTENT);
    assert_eq!(get(&app, &id).await["starred"], false);
    assert!(starred_ids(&app, "").await.is_empty());
}

#[tokio::test]
async fn a_review_is_starred_when_any_of_its_rounds_is() {
    let app = app();
    let first = send(&app, review("Round one")).await;
    assert_eq!(star(&app, "PUT", &first).await, StatusCode::NO_CONTENT);
    let mut next = review("Round two");
    next["revises"] = json!(first);
    let second = send(&app, next).await;

    // the newest round shows the review as starred, with no star of its own
    let newest = get(&app, &second).await;
    assert_eq!(newest["starred"], true);
    assert_eq!(newest["starred_at"], Value::Null);
    // the listing, which shows the newest round, finds it
    assert_eq!(starred_ids(&app, "").await, std::slice::from_ref(&second));
    // listing every round, as the History does, finds only the round
    // that holds the star
    assert_eq!(
        starred_ids(&app, "&include_revised=true").await,
        std::slice::from_ref(&first)
    );
}

#[tokio::test]
async fn a_question_or_an_unknown_review_cannot_be_starred() {
    let app = app();
    let question = send(
        &app,
        json!({
            "kind": "question",
            "title": "Ship it?",
            "payload": {"questions": [{"id": "q1", "question": "Ship it?", "answer": "yes-no"}]}
        }),
    )
    .await;
    assert_eq!(
        star(&app, "PUT", &question).await,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(star(&app, "PUT", "r_nope").await, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_star_is_announced_and_is_not_activity_in_the_session() {
    let app = app();
    let mut sent = review("In a session");
    sent["requested_by"] = json!("claude-code");
    sent["session"] = json!("s-star");
    let id = send(&app, sent).await;
    let before =
        call(&app, "GET", "/api/v1/sessions", None).await.1["sessions"][0]["last_at"].clone();

    assert_eq!(star(&app, "PUT", &id).await, StatusCode::NO_CONTENT);
    assert_eq!(star(&app, "DELETE", &id).await, StatusCode::NO_CONTENT);
    let (_, events) = call(&app, "GET", &format!("/api/v1/reviews/{id}/events"), None).await;
    let kinds: Vec<&str> = events
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["created", "starred", "unstarred"]);
    let after =
        call(&app, "GET", "/api/v1/sessions", None).await.1["sessions"][0]["last_at"].clone();
    assert_eq!(before, after);
}
