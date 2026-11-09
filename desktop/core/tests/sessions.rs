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
async fn the_listing_searches_titles_and_reports_the_newest_status() {
    let app = app();
    send(
        &app,
        question("claude-code", Some("s-search"), "Deploy the quokka?"),
    )
    .await;
    send(
        &app,
        question("claude-code", Some("s-search"), "Second question"),
    )
    .await;
    status(
        &app,
        "claude-code",
        "s-search",
        json!({"text": "Port the loader", "kind": "start"}),
    )
    .await;
    status(
        &app,
        "claude-code",
        "s-search",
        json!({"text": "Running the tests"}),
    )
    .await;
    status(
        &app,
        "claude-code",
        "s-search",
        json!({"text": "A note last", "kind": "note"}),
    )
    .await;
    send(&app, question("codex", Some("s-other"), "Something else")).await;

    // a word of a question's title, of the session's title, or of its agent
    for (q, found) in [
        ("quokka", vec!["s-search"]),
        ("loader", vec!["s-search"]),
        ("codex", vec!["s-other"]),
        ("quokka loader", vec!["s-search"]),
        ("quokka codex", vec![]),
    ] {
        assert_eq!(
            listed_ids(&app, &format!("?q={}", q.replace(' ', "+"))).await,
            found,
            "{q}"
        );
    }
    // each session counts each question once
    let (_, listing) = call(&app, "GET", "/api/v1/sessions?q=quokka", None).await;
    let session = &listing["sessions"][0];
    assert_eq!(session["total"]["questions"], 2);
    assert_eq!(session["waiting"]["questions"], 2);
    assert_eq!(session["status"]["text"], "Running the tests");
    let (_, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-search", None).await;
    let newest = thread["statuses"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(newest["text"], "A note last");
    assert_eq!(session["reported_at"], newest["at"]);
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
    assert_eq!(code, StatusCode::OK);
    send(&app, question("claude-code", Some("s-1"), "Use SQLite?")).await;
    status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": "Running the tests"}),
    )
    .await;
    let (code, _) = status(&app, "claude-code", "s-1", json!({"kind": "done"})).await;
    assert_eq!(code, StatusCode::OK);

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
    assert_eq!(code, StatusCode::OK);
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
async fn statuses_and_reviews_sent_within_one_second_have_times_in_their_order() {
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
    let status_at = |i: usize| thread["statuses"][i]["at"].as_str().unwrap().to_string();
    let review_at = |id: &str| {
        thread["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap()["created_at"]
            .as_str()
            .unwrap()
            .to_string()
    };
    // as text, as the thread sorts them
    let times = [
        status_at(0),
        review_at(&question),
        status_at(1),
        review_at(&review),
        status_at(2),
    ];
    assert!(times.windows(2).all(|w| w[0] < w[1]), "{times:?}");
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
                assert_eq!(code, StatusCode::OK);
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
    assert_eq!(code, StatusCode::OK);

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
    assert_eq!(code, StatusCode::OK);
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

#[tokio::test]
async fn a_session_is_read_with_every_round_of_its_reviews() {
    let app = app();
    let first = send(&app, review("claude-code", Some("s-1"), "The schema")).await;
    let asked = send(&app, question("claude-code", Some("s-1"), "Ship it?")).await;
    let mut next = review("claude-code", Some("s-1"), "The schema");
    next["revises"] = json!(first);
    let second = send(&app, next).await;

    let (status, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
    assert_eq!(status, StatusCode::OK, "{thread}");
    let ids: Vec<&str> = thread["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [first.as_str(), asked.as_str(), second.as_str()]);
    assert_eq!(thread["items"][2]["revises"], first.as_str());
    // a review counts once, however many rounds it has
    assert_eq!(
        thread["session"]["total"],
        json!({"questions": 1, "reviews": 1})
    );
}

async fn message(app: &App, session: &str, text: &str) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        &format!("/api/v1/sessions/claude-code/{session}/messages"),
        Some(json!({ "text": text })),
    )
    .await
}

async fn delete_message(app: &App, session: &str, id: &Value) -> StatusCode {
    call(
        app,
        "DELETE",
        &format!("/api/v1/sessions/claude-code/{session}/messages/{id}"),
        None,
    )
    .await
    .0
}

#[tokio::test]
async fn a_status_delivers_the_queued_messages_once_oldest_first() {
    let app = app();
    status(&app, "claude-code", "s-1", json!({"text": "Building"})).await;
    let (code, first) = message(&app, "s-1", "  Use the v2 endpoint.  ").await;
    assert_eq!(code, StatusCode::CREATED, "{first}");
    assert_eq!(first["text"], "Use the v2 endpoint.");
    assert_eq!(first["delivered_at"], Value::Null);
    let (_, second) = message(&app, "s-1", "And keep the old tests.").await;
    // another session's message stays where it is
    status(&app, "claude-code", "s-2", json!({"text": "Elsewhere"})).await;
    message(&app, "s-2", "Not for s-1.").await;

    let (_, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
    let texts: Vec<&str> = thread["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, ["Use the v2 endpoint.", "And keep the old tests."]);

    let (code, delivered) = status(&app, "claude-code", "s-1", json!({"text": "Testing"})).await;
    assert_eq!(code, StatusCode::OK);
    let ids: Vec<&Value> = delivered["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| &m["id"])
        .collect();
    assert_eq!(ids, [&first["id"], &second["id"]]);
    assert!(delivered["messages"][0]["delivered_at"].is_string());
    // once: the next status has nothing for the agent
    let (_, again) = status(&app, "claude-code", "s-1", json!({"kind": "done"})).await;
    assert_eq!(again["messages"], json!([]));

    let (_, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
    assert!(thread["messages"][1]["delivered_at"].is_string());
    let (_, other) = call(&app, "GET", "/api/v1/sessions/claude-code/s-2", None).await;
    assert_eq!(other["messages"][0]["delivered_at"], Value::Null);
}

#[tokio::test]
async fn a_queued_message_can_be_deleted_and_a_delivered_one_cannot() {
    let app = app();
    status(&app, "claude-code", "s-1", json!({"text": "Building"})).await;
    let (_, queued) = message(&app, "s-1", "Never mind.").await;
    assert_eq!(
        delete_message(&app, "s-1", &queued["id"]).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        delete_message(&app, "s-1", &queued["id"]).await,
        StatusCode::NOT_FOUND
    );

    let (_, sent) = message(&app, "s-1", "Use the v2 endpoint.").await;
    // a message is deleted only from its own session
    status(&app, "claude-code", "s-2", json!({"text": "Elsewhere"})).await;
    assert_eq!(
        delete_message(&app, "s-2", &sent["id"]).await,
        StatusCode::NOT_FOUND
    );
    status(&app, "claude-code", "s-1", json!({"text": "Testing"})).await;
    assert_eq!(
        delete_message(&app, "s-1", &sent["id"]).await,
        StatusCode::CONFLICT
    );

    let (_, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
    assert_eq!(thread["messages"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_message_needs_a_session_and_its_text() {
    let app = app();
    let (code, _) = message(&app, "s-nope", "Anyone?").await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    status(&app, "claude-code", "s-1", json!({"text": "Building"})).await;
    for text in ["   ".to_string(), "é".repeat(4001)] {
        let (code, refused) = message(&app, "s-1", &text).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    }
    let (code, _) = message(&app, "s-1", &"é".repeat(4000)).await;
    assert_eq!(code, StatusCode::CREATED);
}

#[tokio::test]
async fn messages_are_announced_when_sent_delivered_and_deleted() {
    let app = app();
    status(&app, "claude-code", "s-1", json!({"text": "Building"})).await;
    let mut notices = app.state.events().subscribe();
    let (_, sent) = message(&app, "s-1", "Use the v2 endpoint.").await;
    let notice = notices.recv().await.unwrap();
    assert_eq!(notice.kind, "session_message_sent");
    assert_eq!(notice.session.unwrap()["message"]["id"], sent["id"]);

    let (_, other) = message(&app, "s-1", "Never mind.").await;
    notices.recv().await.unwrap();
    delete_message(&app, "s-1", &other["id"]).await;
    let notice = notices.recv().await.unwrap();
    assert_eq!(notice.kind, "session_message_deleted");
    assert_eq!(notice.session.unwrap()["message"]["id"], other["id"]);

    status(&app, "claude-code", "s-1", json!({"text": "Testing"})).await;
    assert_eq!(notices.recv().await.unwrap().kind, "session_status");
    let notice = notices.recv().await.unwrap();
    assert_eq!(notice.kind, "session_messages_delivered");
    assert_eq!(notice.session.unwrap()["messages"][0]["id"], sent["id"]);
}

#[tokio::test]
async fn a_queued_message_keeps_its_session_from_the_sweep() {
    let app = app();
    status(&app, "claude-code", "s-queued", json!({"text": "Planning"})).await;
    message(&app, "s-queued", "Use the v2 endpoint.").await;
    status(
        &app,
        "claude-code",
        "s-delivered",
        json!({"text": "Planning"}),
    )
    .await;
    message(&app, "s-delivered", "Use the v2 endpoint.").await;
    status(
        &app,
        "claude-code",
        "s-delivered",
        json!({"text": "Received"}),
    )
    .await;
    let later = chrono::Utc::now() + chrono::Duration::days(1);
    app.state.reviews().sweep_history_before(later).unwrap();
    let (_, listing) = call(&app, "GET", "/api/v1/sessions", None).await;
    let ids: Vec<&str> = listing["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["s-queued"]);
}

#[tokio::test]
async fn a_message_sent_between_two_statuses_has_a_time_between_theirs() {
    let app = app();
    status(&app, "claude-code", "s-1", json!({"text": "Reading"})).await;
    let (_, sent) = message(&app, "s-1", "Use the v2 endpoint.").await;
    status(&app, "claude-code", "s-1", json!({"text": "Switching"})).await;
    let (_, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
    let reading = thread["statuses"][0]["at"].as_str().unwrap();
    let switching = thread["statuses"][1]["at"].as_str().unwrap();
    let sent = sent["sent_at"].as_str().unwrap();
    assert_eq!(thread["messages"][0]["sent_at"], sent);
    assert!(
        reading < sent && sent < switching,
        "{reading} {sent} {switching}"
    );
}

#[tokio::test]
async fn a_status_answers_with_the_open_stage() {
    let app = app();
    // no stage was started yet
    let (_, answer) = status(&app, "claude-code", "s-1", json!({"text": "Reading"})).await;
    assert_eq!(answer["stage"], Value::Null);

    let (_, answer) = status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": "Add rate limiting", "kind": "start"}),
    )
    .await;
    assert_eq!(answer["stage"]["text"], "Add rate limiting");
    assert!(answer["stage"]["at"].is_string());

    // a step and a note stay in the stage
    let (_, answer) = status(&app, "claude-code", "s-1", json!({"text": "Testing"})).await;
    assert_eq!(answer["stage"]["text"], "Add rate limiting");
    let (_, answer) = status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": "A summary", "kind": "note"}),
    )
    .await;
    assert_eq!(answer["stage"]["text"], "Add rate limiting");

    // the newest stage is the open one
    let (_, answer) = status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": "Fix the tests", "kind": "start"}),
    )
    .await;
    assert_eq!(answer["stage"]["text"], "Fix the tests");

    // finishing closes it, and a note does not open it again
    let (_, answer) = status(&app, "claude-code", "s-1", json!({"kind": "done"})).await;
    assert_eq!(answer["stage"], Value::Null);
    let (_, answer) = status(
        &app,
        "claude-code",
        "s-1",
        json!({"text": "A last word", "kind": "note"}),
    )
    .await;
    assert_eq!(answer["stage"], Value::Null);

    // a step after the end says the agent is working again
    let (_, answer) = status(&app, "claude-code", "s-1", json!({"text": "One more"})).await;
    assert_eq!(answer["stage"]["text"], "Fix the tests");

    // another session has its own
    let (_, answer) = status(&app, "claude-code", "s-2", json!({"text": "Elsewhere"})).await;
    assert_eq!(answer["stage"], Value::Null);
}

#[tokio::test]
async fn a_status_is_refused_while_sessions_are_off() {
    let app = common::app_as_new();
    let (code, body) = status(&app, "claude-code", "s-1", json!({"text": "Building"})).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "off");
}
