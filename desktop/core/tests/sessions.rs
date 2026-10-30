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

async fn status(app: &App, agent: &str, session: &str, body: Value) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        &format!("/api/v1/sessions/{agent}/{session}/status"),
        Some(body),
    )
    .await
}

#[tokio::test]
async fn statuses_are_kept_in_order_and_the_newest_is_listed() {
    let app = app();
    // a status before anything else makes the session
    let (code, _) = status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": "Add rate limiting", "kind": "start", "origin": {"repo": "acme", "ref": "main", "folder": "/work/acme"}}),
    )
    .await;
    assert_eq!(code, StatusCode::NO_CONTENT);
    send(&app, question("claude-code", Some("s-1"), "Use SQLite?")).await;
    status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": "Running the tests"}),
    )
    .await;
    let (code, _) = status(&app, "claude-code", "s-1", json!({"kind": "done"})).await;
    assert_eq!(code, StatusCode::NO_CONTENT);

    let (_, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
    let statuses: Vec<(String, String)> = thread["statuses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["text"].as_str().unwrap().into(),
                s["kind"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        statuses,
        [
            ("Add rate limiting".into(), "start".into()),
            ("Running the tests".into(), "step".into()),
            (String::new(), "done".into()),
        ]
    );
    // the folder the status gave stays when the question gives none
    assert_eq!(thread["session"]["folder"], "/work/acme");
    assert_eq!(thread["session"]["status"]["kind"], "done");
    // the task started names the session
    assert_eq!(thread["session"]["title"], "Add rate limiting");

    let (_, listing) = call(&app, "GET", "/api/v1/sessions", None).await;
    assert_eq!(listing["sessions"][0]["status"]["kind"], "done");
    assert_eq!(listing["sessions"][0]["title"], "Add rate limiting");
    assert_eq!(listing["sessions"][0]["total"]["questions"], 1);

    // a status after the end starts the work again
    status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": "Fixing a review comment"}),
    )
    .await;
    let (_, listing) = call(&app, "GET", "/api/v1/sessions", None).await;
    assert_eq!(
        listing["sessions"][0]["status"]["text"],
        "Fixing a review comment"
    );
    assert_eq!(listing["sessions"][0]["status"]["kind"], "step");

    // a new task renames the session, and the search finds it
    status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": "Document the limits", "kind": "start"}),
    )
    .await;
    let (_, found) = call(&app, "GET", "/api/v1/sessions?q=document", None).await;
    assert_eq!(found["total"], 1);
    assert_eq!(found["sessions"][0]["title"], "Document the limits");
}

#[tokio::test]
async fn a_session_with_only_statuses_is_listed_with_nothing_in_it() {
    let app = app();
    status(&app, "codex", "s-quiet", json!({"text": "Planning"})).await;
    let (_, listing) = call(&app, "GET", "/api/v1/sessions", None).await;
    assert_eq!(listing["total"], 1);
    assert_eq!(
        listing["sessions"][0]["total"],
        json!({"questions": 0, "reviews": 0})
    );
    assert_eq!(
        listing["sessions"][0]["waiting"],
        json!({"questions": 0, "reviews": 0})
    );
    let (_, waiting) = call(&app, "GET", "/api/v1/sessions?waiting=true", None).await;
    assert_eq!(waiting["total"], 0);
    let (code, thread) = call(&app, "GET", "/api/v1/sessions/codex/s-quiet", None).await;
    assert_eq!(code, StatusCode::OK, "{thread}");
    assert_eq!(thread["items"], json!([]));
}

#[tokio::test]
async fn a_status_is_one_short_line() {
    let app = app();
    for body in [
        json!({"text": ""}),
        json!({"text": "   "}),
        json!({"text": "two\nlines"}),
        json!({"text": "x".repeat(201)}),
        json!({"text": "", "kind": "start"}),
        json!({"text": "Something", "kind": "paused"}),
    ] {
        let (code, refused) = status(&app, "claude-code", "s-1", body.clone()).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{body} {refused}");
    }
    let (code, _) = status(&app, "claude-code", "s-1", json!({"text": "é".repeat(200)})).await;
    assert_eq!(code, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_status_is_announced_with_its_session() {
    let app = app();
    let mut notices = app.state.events().subscribe();
    status(&app, "claude-code", "s-1", json!({"text": "Building"})).await;
    let notice = notices.recv().await.unwrap();
    assert_eq!(notice.kind, "session_status");
    let session = notice.session.unwrap();
    assert_eq!(session["agent"], "claude-code");
    assert_eq!(session["id"], "s-1");
    assert_eq!(session["status"]["text"], "Building");
    // and the same to a client catching up
    let caught = app.state.events().after(notice.event_id - 1, 10).unwrap();
    assert_eq!(
        caught[0].session.as_ref().unwrap()["status"]["text"],
        "Building"
    );
}

#[tokio::test]
async fn the_history_sweep_deletes_idle_sessions_with_nothing_left() {
    let app = app();
    status(&app, "codex", "s-idle", json!({"text": "Planning"})).await;
    send(&app, question("claude-code", Some("s-kept"), "Pending?")).await;
    let later = chrono::Utc::now() + chrono::Duration::days(1);
    app.state.reviews().sweep_history_before(later).unwrap();
    let (_, listing) = call(&app, "GET", "/api/v1/sessions", None).await;
    let ids: Vec<&str> = listing["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    // the pending question keeps its session
    assert_eq!(ids, ["s-kept"]);
}

#[tokio::test]
async fn a_status_names_the_review_it_came_after_within_the_same_second() {
    let app = app();
    status(&app, "claude-code", "s-1", json!({"text": "Starting"})).await;
    let question = send(&app, question("claude-code", Some("s-1"), "Use SQLite?")).await;
    status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": "Sent a question"}),
    )
    .await;
    let review = send(&app, review("claude-code", Some("s-1"), "The schema")).await;
    status(&app, "claude-code", "s-1", json!({"text": "Sent a review"})).await;

    let (_, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
    let after: Vec<Value> = thread["statuses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["after"].clone())
        .collect();
    assert_eq!(after, [Value::Null, json!(question), json!(review)]);
}

#[tokio::test]
async fn sessions_are_filtered_by_project_and_list_their_projects() {
    let app = app();
    send(&app, question("claude-code", Some("s-acme"), "In acme?")).await;
    let mut other = question("claude-code", Some("s-web"), "In web?");
    other["origin"] = json!({"repo": "acme/web"});
    send(&app, other).await;
    let mut none = question("codex", Some("s-none"), "Nowhere?");
    none["origin"] = json!({});
    send(&app, none).await;

    let ids = |listed: &Value| -> Vec<String> {
        let mut ids: Vec<String> = listed["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["id"].as_str().unwrap().to_string())
            .collect();
        ids.sort();
        ids
    };
    let (_, listed) = call(&app, "GET", "/api/v1/sessions?repo=acme/web", None).await;
    assert_eq!(ids(&listed), ["s-web"]);
    assert_eq!(listed["total"], 1);
    // "-" stands for the sessions with no project
    let (_, listed) = call(&app, "GET", "/api/v1/sessions?repo=-", None).await;
    assert_eq!(ids(&listed), ["s-none"]);
    // the projects to filter by are every session's, whatever the filter
    assert_eq!(listed["facets"]["repos"], json!(["acme", "acme/web"]));
    assert_eq!(listed["facets"]["unassigned"], true);
}
