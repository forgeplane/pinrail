//! An agent's sessions, as the reviews and questions they sent make them:
//! each named by its agent and its session id, listed with what waits in
//! it, and read whole as a thread.

use axum::http::StatusCode;
use serde_json::{Value, json};

mod common;
use common::{App, app, call};

fn question(agent: &str, session: Option<&str>, text: &str) -> Value {
    json!({
        "kind": "question",
        "title": text,
        "origin": {"repo": "acme", "ref": "main"},
        "requested_by": agent,
        "session": session,
        "payload": {"questions": [{"id": "q1", "question": text, "answer": "yes-no"}]}
    })
}

fn review(agent: &str, session: Option<&str>, title: &str) -> Value {
    json!({
        "plugin": "list",
        "title": title,
        "origin": {"repo": "acme", "ref": "feature"},
        "requested_by": agent,
        "session": session,
        "payload": {"groups": [{"title": "g", "items": [{"id": 1, "title": "x"}]}]}
    })
}

async fn send(app: &App, body: Value) -> String {
    let (status, sent) = call(app, "POST", "/api/v1/reviews", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{sent}");
    sent["id"].as_str().unwrap().to_string()
}

async fn answer(app: &App, id: &str) {
    let data = json!({"data": {"answers": [{"id": "q1", "yes": true}]}});
    let (status, decided) = call(
        app,
        "POST",
        &format!("/api/v1/reviews/{id}/decision"),
        Some(data),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decided}");
}

#[tokio::test]
async fn a_session_is_read_as_a_thread_of_its_questions_and_reviews() {
    let app = app();
    let first = send(&app, question("claude-code", Some("s-1"), "Use SQLite?")).await;
    let second = send(&app, review("claude-code", Some("s-1"), "The schema")).await;
    let third = send(&app, question("claude-code", Some("s-1"), "Ship it?")).await;
    // another agent with the same id, and the same agent in another session
    send(&app, question("codex", Some("s-1"), "Elsewhere?")).await;
    send(&app, question("claude-code", Some("s-2"), "Other?")).await;
    answer(&app, &first).await;

    let (status, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
    assert_eq!(status, StatusCode::OK, "{thread}");
    let ids: Vec<&str> = thread["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [first.as_str(), second.as_str(), third.as_str()]);
    // the questions come with what they ask and how they were answered
    assert_eq!(
        thread["items"][0]["payload"]["questions"][0]["question"],
        "Use SQLite?"
    );
    assert_eq!(
        thread["items"][0]["decision"]["data"]["answers"][0]["yes"],
        true
    );
    // a review's payload stays with the review, which the thread opens
    assert_eq!(thread["items"][1]["payload"], Value::Null);
    assert_eq!(thread["items"][1]["kind"], "review");

    let session = &thread["session"];
    assert_eq!(session["agent"], "claude-code");
    assert_eq!(session["id"], "s-1");
    // where the session worked last
    assert_eq!(session["repo"], "acme");
    assert_eq!(session["ref"], "main");
    assert_eq!(session["total"], json!({"questions": 2, "reviews": 1}));
    assert_eq!(session["waiting"], json!({"questions": 1, "reviews": 1}));
    assert!(session["first_at"].as_str().unwrap() <= session["last_at"].as_str().unwrap());
}

#[tokio::test]
async fn sessions_are_listed_with_those_waiting_first() {
    let app = app();
    let done = send(&app, question("claude-code", Some("s-done"), "Done?")).await;
    send(&app, question("codex", Some("s-waiting"), "Waiting?")).await;
    send(&app, review("gemini", Some("s-later"), "Later")).await;
    answer(&app, &done).await;
    let gemini = call(&app, "GET", "/api/v1/reviews?plugin=list", None)
        .await
        .1["reviews"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/reviews/{gemini}/withdraw"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // sent without a session: in no session
    send(&app, question("claude-code", None, "Alone?")).await;

    let (status, listing) = call(&app, "GET", "/api/v1/sessions", None).await;
    assert_eq!(status, StatusCode::OK, "{listing}");
    let names: Vec<(String, String)> = listing["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["agent"].as_str().unwrap().into(),
                s["id"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        names,
        [
            ("codex".into(), "s-waiting".into()),
            ("gemini".into(), "s-later".into()),
            ("claude-code".into(), "s-done".into()),
        ]
    );
    assert_eq!(listing["total"], 3);

    let (_, waiting) = call(&app, "GET", "/api/v1/sessions?waiting=true", None).await;
    assert_eq!(waiting["total"], 1);
    let (_, by_agent) = call(&app, "GET", "/api/v1/sessions?agent=gemini", None).await;
    assert_eq!(by_agent["sessions"][0]["id"], "s-later");
    let (_, paged) = call(&app, "GET", "/api/v1/sessions?limit=1&offset=1", None).await;
    assert_eq!(paged["sessions"][0]["id"], "s-later");
    assert_eq!(paged["total"], 3);
}

#[tokio::test]
async fn a_session_nobody_sent_from_is_404() {
    let app = app();
    let (status, _) = call(&app, "GET", "/api/v1/sessions/claude-code/nothing", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn reviews_are_listed_by_session() {
    let app = app();
    let mine = send(&app, question("claude-code", Some("s-1"), "Mine?")).await;
    send(&app, question("codex", Some("s-1"), "Theirs?")).await;
    let (_, listing) = call(
        &app,
        "GET",
        "/api/v1/reviews?agent=claude-code&session=s-1",
        None,
    )
    .await;
    let ids: Vec<&str> = listing["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [mine.as_str()]);
}
