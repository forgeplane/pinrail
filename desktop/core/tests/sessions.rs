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

#[tokio::test]
async fn stages_are_searched_by_name_across_sessions() {
    let app = app();
    let start = |text: &str| json!({"text": text, "kind": "start", "origin": {"repo": "acme"}});
    status(&app, "claude-code", "s-a", start("Add rate limiting")).await;
    status(
        &app,
        "claude-code",
        "s-a",
        json!({"text": "Running the tests"}),
    )
    .await;
    status(&app, "claude-code", "s-a", start("Document the limits")).await;
    status(&app, "codex", "s-b", start("Fix the rate of retries")).await;

    let (status_code, found) = call(&app, "GET", "/api/v1/stages?q=rate", None).await;
    assert_eq!(status_code, StatusCode::OK, "{found}");
    let stages = found["stages"].as_array().unwrap();
    // only the starts whose name matches, newest first, with their session
    let names: Vec<&str> = stages.iter().map(|s| s["text"].as_str().unwrap()).collect();
    assert_eq!(names, ["Fix the rate of retries", "Add rate limiting"]);
    assert_eq!(stages[1]["agent"], "claude-code");
    assert_eq!(stages[1]["session"], "s-a");
    assert_eq!(stages[1]["repo"], "acme");
    assert!(stages[1]["id"].is_i64());

    // no words: the newest stages, as many as asked
    let (_, found) = call(&app, "GET", "/api/v1/stages?limit=1", None).await;
    assert_eq!(found["stages"][0]["text"], "Fix the rate of retries");
    assert_eq!(found["stages"].as_array().unwrap().len(), 1);
}

async fn archive(app: &App, method: &str, agent: &str, session: &str) -> StatusCode {
    call(
        app,
        method,
        &format!("/api/v1/sessions/{agent}/{session}/archive"),
        None,
    )
    .await
    .0
}

async fn listed_ids(app: &App, query: &str) -> Vec<String> {
    let (_, listed) = call(app, "GET", &format!("/api/v1/sessions{query}"), None).await;
    let mut ids: Vec<String> = listed["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn an_archived_session_leaves_the_listing_until_asked_for() {
    let app = app();
    send(&app, question("claude-code", Some("s-keep"), "Keep?")).await;
    send(&app, question("claude-code", Some("s-old"), "Old?")).await;
    let before = call(&app, "GET", "/api/v1/sessions/claude-code/s-old", None)
        .await
        .1["session"]["last_at"]
        .clone();

    assert_eq!(
        archive(&app, "PUT", "claude-code", "s-old").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        archive(&app, "PUT", "claude-code", "s-old").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(listed_ids(&app, "").await, ["s-keep"]);
    assert_eq!(
        listed_ids(&app, "?include_archived=true").await,
        ["s-keep", "s-old"]
    );
    // its thread still opens, and says when it was archived
    let (_, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-old", None).await;
    assert!(thread["session"]["archived_at"].is_string(), "{thread}");
    // archiving is not activity
    assert_eq!(thread["session"]["last_at"], before);

    assert_eq!(
        archive(&app, "DELETE", "claude-code", "s-old").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(listed_ids(&app, "").await, ["s-keep", "s-old"]);
    assert_eq!(
        archive(&app, "PUT", "claude-code", "s-nope").await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_question_a_review_or_a_status_brings_an_archived_session_back() {
    let app = app();
    send(&app, question("claude-code", Some("s-1"), "First?")).await;
    for (i, step) in ["question", "review", "status"].iter().enumerate() {
        assert_eq!(
            archive(&app, "PUT", "claude-code", "s-1").await,
            StatusCode::NO_CONTENT
        );
        assert!(
            listed_ids(&app, "").await.is_empty(),
            "archived before the {step}"
        );
        match *step {
            "question" => drop(send(&app, question("claude-code", Some("s-1"), "Again?")).await),
            "review" => {
                drop(send(&app, review("claude-code", Some("s-1"), &format!("R{i}"))).await)
            }
            _ => {
                let (code, _) =
                    status(&app, "claude-code", "s-1", json!({"text": "Working"})).await;
                assert_eq!(code, StatusCode::NO_CONTENT);
            }
        }
        assert_eq!(listed_ids(&app, "").await, ["s-1"], "back after the {step}");
    }
}

#[tokio::test]
async fn archiving_is_announced_with_its_session() {
    let app = app();
    send(&app, question("claude-code", Some("s-1"), "First?")).await;
    let mut notices = app.state.events().subscribe();
    assert_eq!(
        archive(&app, "PUT", "claude-code", "s-1").await,
        StatusCode::NO_CONTENT
    );
    let notice = notices.recv().await.unwrap();
    assert_eq!(notice.kind, "session_archived");
    assert_eq!(notice.session.unwrap()["id"], "s-1");
    assert_eq!(
        archive(&app, "DELETE", "claude-code", "s-1").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(notices.recv().await.unwrap().kind, "session_unarchived");
}

#[tokio::test]
async fn a_note_is_kept_with_its_markdown_and_is_not_the_latest_status() {
    let app = app();
    let markdown = "## What changed\n\n- the limits\n- their docs";
    status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": "Add rate limiting", "kind": "start"}),
    )
    .await;
    status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": "Running the tests"}),
    )
    .await;
    let (code, _) = status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": markdown, "kind": "note"}),
    )
    .await;
    assert_eq!(code, StatusCode::NO_CONTENT);

    let (_, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
    let last = thread["statuses"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["kind"], "note");
    assert_eq!(last["text"], markdown);
    // the latest status is still the step, and the note is the newest report
    assert_eq!(thread["session"]["status"]["text"], "Running the tests");
    assert_eq!(thread["session"]["reported_at"], last["at"]);
    assert_eq!(thread["session"]["title"], "Add rate limiting");

    // a note after the end leaves the session finished
    status(&app, "claude-code", "s-1", json!({"kind": "done"})).await;
    status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": "Pushed the fix.", "kind": "note"}),
    )
    .await;
    let (_, listing) = call(&app, "GET", "/api/v1/sessions", None).await;
    assert_eq!(listing["sessions"][0]["status"]["kind"], "done");
}

#[tokio::test]
async fn a_note_is_markdown_of_at_most_ten_thousand_characters() {
    let app = app();
    for text in ["", "  \n ", &"x".repeat(10_001)] {
        let body = json!({"text": text, "kind": "note"});
        let (code, refused) = status(&app, "claude-code", "s-1", body).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    }
    let long = format!("# Summary\n\n{}", "é".repeat(9_989));
    assert_eq!(long.chars().count(), 10_000);
    let (code, _) = status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": long, "kind": "note"}),
    )
    .await;
    assert_eq!(code, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_note_is_announced_without_its_markdown() {
    let app = app();
    let mut notices = app.state.events().subscribe();
    status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": "A long summary", "kind": "note"}),
    )
    .await;
    let notice = notices.recv().await.unwrap();
    assert_eq!(notice.kind, "session_status");
    let session = notice.session.unwrap();
    assert_eq!(session["status"]["kind"], "note");
    // the thread reads it when the notice says there is one
    assert_eq!(session["status"]["text"], "");
}
