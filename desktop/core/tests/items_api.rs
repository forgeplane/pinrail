//! Work items through the API: adding and reading them, claims, what done
//! means, comments with their files, the reviews and sessions that belong
//! to an item, and its timeline.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

mod common;
use common::{App, app, call};

const REPO: &str = "forgeplane/pinrail";

/// A request as an agent sends it, from outside the app's window, naming
/// itself and its session in the body.
async fn agent(
    app: &App,
    method: &str,
    path: &str,
    session: &str,
    body: Value,
) -> (StatusCode, Value) {
    let mut body = body;
    body["by"] = json!("claude-code");
    body["session"] = json!(session);
    send(app, method, path, Some(body)).await
}

/// A request from outside the app's window, as is.
async fn send(app: &App, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
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

/// Adds an item as an agent and returns its key.
async fn add(app: &App, title: &str) -> String {
    add_to(app, REPO, title).await
}

async fn add_to(app: &App, repo: &str, title: &str) -> String {
    let (code, item) = agent(
        app,
        "POST",
        "/api/v1/items",
        "s-1",
        json!({"repo": repo, "title": title}),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{item}");
    item["key"].as_str().unwrap().to_string()
}

async fn item(app: &App, key: &str) -> Value {
    let (code, item) = call(app, "GET", &format!("/api/v1/items/{key}"), None).await;
    assert_eq!(code, StatusCode::OK, "{item}");
    item
}

async fn edit(app: &App, key: &str, session: &str, body: Value) -> (StatusCode, Value) {
    agent(app, "PATCH", &format!("/api/v1/items/{key}"), session, body).await
}

async fn done(app: &App, key: &str, session: &str, comment: &str) -> (StatusCode, Value) {
    agent(
        app,
        "POST",
        &format!("/api/v1/items/{key}/done"),
        session,
        json!({"comment": comment}),
    )
    .await
}

async fn next(app: &App, session: &str, claim: bool) -> Value {
    let (code, answer) = agent(
        app,
        "POST",
        "/api/v1/items/next",
        session,
        json!({"repo": REPO, "claim": claim}),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{answer}");
    answer["item"].clone()
}

fn review(session: &str, title: &str, items: Option<Value>) -> Value {
    let mut body = json!({
        "plugin": "list",
        "title": title,
        "origin": {"repo": REPO, "ref": "main"},
        "requested_by": "claude-code",
        "session": session,
        "payload": {"groups": [{"title": "g", "items": [{"id": 1, "title": "x"}]}]}
    });
    if let Some(items) = items {
        body["items"] = items;
    }
    body
}

async fn submit(app: &App, body: Value) -> (StatusCode, Value) {
    call(app, "POST", "/api/v1/reviews", Some(body)).await
}

async fn decide(app: &App, id: &str) {
    let (code, answer) = call(
        app,
        "POST",
        &format!("/api/v1/reviews/{id}/decision"),
        Some(json!({"data": {"decisions": [{"id": 1, "action": "accept"}], "undecided": []}})),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{answer}");
}

fn keys(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|i| i["key"].as_str().unwrap().to_string())
        .collect()
}

fn timeline_kinds(item: &Value) -> Vec<String> {
    item["timeline"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            e["kind"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| e["type"].as_str().unwrap().to_string())
        })
        .collect()
}

// --- adding and reading

#[tokio::test]
async fn an_agent_adds_an_item_and_reads_it_back() {
    let app = app();
    let (code, added) = agent(
        &app,
        "POST",
        "/api/v1/items",
        "s-1",
        json!({"repo": REPO, "title": "  Load themes  ", "body": "Read the theme files.",
               "link": "https://github.com/forgeplane/pinrail/issues/9"}),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{added}");
    assert_eq!(added["key"], "PIN-1");
    assert_eq!(added["title"], "Load themes");
    assert_eq!(added["status"], "open");
    assert_eq!(added["ready"], true);
    assert_eq!(added["created_by"], "claude-code");
    assert_eq!(added["created_session"], "s-1");
    assert_eq!(timeline_kinds(&added), ["created"]);

    // by its key, by its number in its repository, and by its id
    for path in [
        "/api/v1/items/PIN-1".to_string(),
        "/api/v1/items/1?repo=forgeplane%2Fpinrail".to_string(),
        format!("/api/v1/items/{}", added["id"].as_str().unwrap()),
    ] {
        let (code, read) = call(&app, "GET", &path, None).await;
        assert_eq!(code, StatusCode::OK, "{path} {read}");
        assert_eq!(read["key"], "PIN-1");
        assert_eq!(read["body"], "Read the theme files.");
    }
    let (code, refused) = call(&app, "GET", "/api/v1/items/PIN-9", None).await;
    assert_eq!(code, StatusCode::NOT_FOUND, "{refused}");
    let (code, refused) = call(&app, "GET", "/api/v1/items/1", None).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert!(refused["message"].as_str().unwrap().contains("PIN-1"));
}

#[tokio::test]
async fn an_item_needs_a_repository_and_a_one_line_title_of_200_characters_at_most() {
    let app = app();
    for (body, path) in [
        (json!({"title": "No repository"}), "/repo"),
        (json!({"repo": REPO}), "/title"),
        (json!({"repo": REPO, "title": "   "}), "/title"),
        (json!({"repo": REPO, "title": "Two\nlines"}), "/title"),
        (json!({"repo": REPO, "title": "x".repeat(201)}), "/title"),
        (
            json!({"repo": REPO, "title": "Link", "link": "ftp://x"}),
            "/link",
        ),
        (json!({"repo": REPO, "title": 7}), "/title"),
    ] {
        let (code, refused) = agent(&app, "POST", "/api/v1/items", "s-1", body.clone()).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{body} {refused}");
        assert_eq!(refused["violations"][0]["path"], path, "{body} {refused}");
    }
    // nothing was added, so the first item is still number one
    assert_eq!(add(&app, "First").await, "PIN-1");
}

#[tokio::test]
async fn a_breakdown_is_added_whole_through_the_api() {
    let app = app();
    let (code, added) = agent(
        &app,
        "POST",
        "/api/v1/items",
        "s-1",
        json!({"repo": REPO, "items": [
            {"ref": "themes", "title": "Custom themes", "body": "All of it."},
            {"ref": "format", "parent": "themes", "title": "Theme file format"},
            {"ref": "load", "parent": "themes", "title": "Load themes", "depends_on": ["format"]},
        ]}),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{added}");
    let pairs: Vec<(String, String)> = added["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["ref"].as_str().unwrap().into(),
                p["item"]["key"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        [
            ("themes".into(), "PIN-1".into()),
            ("format".into(), "PIN-2".into()),
            ("load".into(), "PIN-3".into())
        ]
    );
    let parent = item(&app, "PIN-1").await;
    assert_eq!(keys(&parent["sub_items_list"]), ["PIN-2", "PIN-3"]);
    assert_eq!(
        parent["sub_items"],
        json!({"total": 2, "closed": 0, "in_progress": 0})
    );
    // a cycle refuses the whole breakdown, with the circle
    let (code, refused) = agent(
        &app,
        "POST",
        "/api/v1/items",
        "s-1",
        json!({"repo": REPO, "items": [
            {"ref": "a", "title": "A", "depends_on": ["b"]},
            {"ref": "b", "title": "B", "depends_on": ["a"]},
        ]}),
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT, "{refused}");
    assert!(
        refused["message"].as_str().unwrap().contains("→"),
        "{refused}"
    );
    let (_, listed) = call(&app, "GET", "/api/v1/items", None).await;
    assert_eq!(keys(&listed["items"]), ["PIN-1", "PIN-2", "PIN-3"]);
}

#[tokio::test]
async fn the_list_filters_by_status_including_ready_and_blocked() {
    let app = app();
    for t in ["Ready", "Blocked", "Blocker", "Done"] {
        add(&app, t).await;
    }
    edit(&app, "PIN-2", "s-1", json!({"depends_on": ["PIN-3"]})).await;
    let (code, _) = done(&app, "PIN-4", "s-1", "Finished").await;
    assert_eq!(code, StatusCode::OK);
    let list = |query: &'static str| {
        let app = &app;
        async move {
            let (code, listed) = call(app, "GET", &format!("/api/v1/items{query}"), None).await;
            assert_eq!(code, StatusCode::OK, "{listed}");
            keys(&listed["items"])
        }
    };
    assert_eq!(list("").await, ["PIN-1", "PIN-2", "PIN-3"]);
    assert_eq!(list("?status=ready").await, ["PIN-1", "PIN-3"]);
    assert_eq!(list("?status=blocked").await, ["PIN-2"]);
    assert_eq!(list("?status=done").await, ["PIN-4"]);
    assert_eq!(list("?status=blocked,done").await, ["PIN-2", "PIN-4"]);
    assert_eq!(list("?q=block").await, ["PIN-2", "PIN-3"]);
    assert_eq!(list("?parent=PIN-1").await, Vec::<String>::new());
    let (code, _) = call(&app, "GET", "/api/v1/items?status=waiting", None).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
}

// --- next

#[tokio::test]
async fn next_offers_what_unblocks_the_most_then_the_oldest() {
    let app = app();
    for t in [
        "Old",
        "Unblocks one",
        "Unblocks two",
        "Waits 1",
        "Waits 2",
        "Waits 3",
    ] {
        add(&app, t).await;
    }
    edit(&app, "PIN-4", "s-1", json!({"depends_on": ["PIN-2"]})).await;
    edit(&app, "PIN-5", "s-1", json!({"depends_on": ["PIN-3"]})).await;
    edit(&app, "PIN-6", "s-1", json!({"depends_on": ["PIN-3"]})).await;
    assert_eq!(next(&app, "s-1", false).await["key"], "PIN-3");
    // without claiming, the answer stays the same
    assert_eq!(next(&app, "s-1", false).await["key"], "PIN-3");
    done(&app, "PIN-3", "s-1", "Done").await;
    assert_eq!(next(&app, "s-1", false).await["key"], "PIN-2");
    done(&app, "PIN-2", "s-1", "Done").await;
    // then the oldest
    assert_eq!(next(&app, "s-1", false).await["key"], "PIN-1");
}

#[tokio::test]
async fn next_offers_the_sub_items_of_what_the_session_holds_first() {
    let app = app();
    for t in ["Older, elsewhere", "Mine", "Its part"] {
        add(&app, t).await;
    }
    edit(&app, "PIN-3", "s-1", json!({"parent": "PIN-2"})).await;
    edit(&app, "PIN-2", "s-1", json!({"claim": "claim"})).await;
    assert_eq!(next(&app, "s-1", false).await["key"], "PIN-3");
    // another session is offered the oldest
    assert_eq!(next(&app, "s-2", false).await["key"], "PIN-1");
}

#[tokio::test]
async fn next_never_offers_blocked_claimed_closed_or_parent_items_nor_another_repository() {
    let app = app();
    for t in ["Blocked", "Blocker", "Parent", "Child"] {
        add(&app, t).await;
    }
    add_to(&app, "elsewhere", "Not here").await;
    edit(&app, "PIN-1", "s-1", json!({"depends_on": ["PIN-2"]})).await;
    edit(&app, "PIN-4", "s-1", json!({"parent": "PIN-3"})).await;
    edit(&app, "PIN-2", "s-2", json!({"claim": "claim"})).await;
    edit(
        &app,
        "PIN-4",
        "s-2",
        json!({"status": "cancelled", "comment": "Not needed"}),
    )
    .await;
    // PIN-3 is ready now that its sub-item is closed
    assert_eq!(next(&app, "s-1", false).await["key"], "PIN-3");
    done(&app, "PIN-3", "s-1", "Done").await;
    // nothing is left: an answer, not an error
    assert_eq!(next(&app, "s-1", true).await, Value::Null);
    let (code, refused) = agent(&app, "POST", "/api/v1/items/next", "s-1", json!({})).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}

// --- claims

#[tokio::test]
async fn next_with_claim_puts_the_item_in_progress_for_the_session() {
    let app = app();
    add(&app, "Work").await;
    let claimed = next(&app, "s-1", true).await;
    assert_eq!(claimed["key"], "PIN-1");
    assert_eq!(claimed["status"], "in_progress");
    assert_eq!(
        claimed["claim"],
        json!({"agent": "claude-code", "session": "s-1"})
    );
    assert_eq!(timeline_kinds(&claimed), ["created", "claimed"]);
    // and it is not offered again
    assert_eq!(next(&app, "s-2", true).await, Value::Null);
}

#[tokio::test]
async fn sessions_claiming_at_once_get_different_items() {
    let app = app();
    for i in 0..6 {
        add(&app, &format!("Item {i}")).await;
    }
    let mut asking = Vec::new();
    for s in 0..10 {
        let router = app.router.clone();
        asking.push(tokio::spawn(async move {
            let body =
                json!({"repo": REPO, "claim": true, "by": "codex", "session": format!("s-{s}")});
            let request = Request::post("/api/v1/items/next")
                .header("host", "127.0.0.1:4747")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            let response = router.oneshot(request).await.unwrap();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            serde_json::from_slice::<Value>(&bytes).unwrap()["item"]["key"].clone()
        }));
    }
    let mut got: Vec<Value> = Vec::new();
    for a in asking {
        got.push(a.await.unwrap());
    }
    let mut claimed: Vec<String> = got
        .iter()
        .filter_map(|k| k.as_str().map(str::to_string))
        .collect();
    claimed.sort();
    assert_eq!(
        claimed,
        ["PIN-1", "PIN-2", "PIN-3", "PIN-4", "PIN-5", "PIN-6"]
    );
    assert_eq!(got.iter().filter(|k| k.is_null()).count(), 4);
}

#[tokio::test]
async fn a_claim_another_session_holds_is_refused_unless_taken() {
    let app = app();
    add(&app, "Work").await;
    let (code, _) = edit(&app, "PIN-1", "s-1", json!({"claim": "claim"})).await;
    assert_eq!(code, StatusCode::OK);
    let (code, refused) = edit(&app, "PIN-1", "s-2", json!({"claim": "claim"})).await;
    assert_eq!(code, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused["message"],
        "PIN-1 is claimed by claude-code (session s-1)"
    );
    // nor can another session release it
    let (code, _) = edit(&app, "PIN-1", "s-2", json!({"claim": "release"})).await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    let (code, taken) = edit(&app, "PIN-1", "s-2", json!({"claim": "take"})).await;
    assert_eq!(code, StatusCode::OK, "{taken}");
    assert_eq!(taken["item"]["claim"]["session"], "s-2");
}

#[tokio::test]
async fn releasing_a_claim_opens_the_item_and_closing_it_ends_the_claim() {
    let app = app();
    add(&app, "Released").await;
    add(&app, "Finished").await;
    edit(&app, "PIN-1", "s-1", json!({"claim": "claim"})).await;
    let (_, released) = edit(&app, "PIN-1", "s-1", json!({"claim": "release"})).await;
    assert_eq!(released["item"]["status"], "open");
    assert_eq!(released["item"]["claim"], Value::Null);

    edit(&app, "PIN-2", "s-1", json!({"claim": "claim"})).await;
    let (_, closed) = done(&app, "PIN-2", "s-1", "Done").await;
    assert_eq!(closed["item"]["claim"], Value::Null);
    // a closed item is not claimed until it is opened again
    let (code, refused) = edit(&app, "PIN-2", "s-1", json!({"claim": "claim"})).await;
    assert_eq!(code, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused["message"],
        "PIN-2 is done: open it again to work on it"
    );
    let (_, reopened) = edit(&app, "PIN-2", "s-1", json!({"status": "open"})).await;
    assert_eq!(reopened["item"]["closed_at"], Value::Null);
}

#[tokio::test]
async fn archiving_a_session_releases_its_claims_and_no_others() {
    let app = app();
    add(&app, "Mine").await;
    add(&app, "Theirs").await;
    for (key, session) in [("PIN-1", "s-1"), ("PIN-2", "s-2")] {
        // the session exists once it has reported a status
        agent(
            &app,
            "POST",
            &format!("/api/v1/sessions/claude-code/{session}/status"),
            session,
            json!({"text": "Working"}),
        )
        .await;
        edit(&app, key, session, json!({"claim": "claim"})).await;
    }
    let (code, _) = call(
        &app,
        "PUT",
        "/api/v1/sessions/claude-code/s-1/archive",
        None,
    )
    .await;
    assert!(code.is_success(), "{code}");
    assert_eq!(item(&app, "PIN-1").await["status"], "open");
    assert_eq!(item(&app, "PIN-2").await["status"], "in_progress");
    let released = item(&app, "PIN-1").await;
    assert_eq!(timeline_kinds(&released).last().unwrap(), "released");
}

// --- done

#[tokio::test]
async fn an_agent_cannot_mark_done_an_item_with_a_pending_review() {
    let app = app();
    add(&app, "Work").await;
    edit(&app, "PIN-1", "s-1", json!({"claim": "claim"})).await;
    let (_, sent) = submit(&app, review("s-1", "Theme loader", None)).await;
    let id = sent["id"].as_str().unwrap();
    let (code, refused) = done(&app, "PIN-1", "s-1", "Shipped").await;
    assert_eq!(code, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused["message"],
        format!("PIN-1 is not done: \"Theme loader\" ({id}) still waits on the person")
    );
    decide(&app, id).await;
    let (code, closed) = done(&app, "PIN-1", "s-1", "Shipped in 3f2a1c").await;
    assert_eq!(code, StatusCode::OK, "{closed}");
    assert_eq!(closed["item"]["status"], "done");
    let last = closed["item"]["timeline"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["type"], "comment");
    assert_eq!(last["body"], "Shipped in 3f2a1c");
}

#[tokio::test]
async fn an_agent_cannot_mark_done_an_item_with_a_pending_question() {
    let app = app();
    add(&app, "Work").await;
    let mut question = json!({
        "kind": "question", "title": "Watch or poll?", "items": ["PIN-1"],
        "origin": {"repo": REPO}, "requested_by": "claude-code", "session": "s-1",
        "payload": {"questions": [{"id": "q1", "question": "Watch or poll?", "answer": "text"}]}
    });
    let (code, asked) = submit(&app, question.take()).await;
    assert_eq!(code, StatusCode::CREATED, "{asked}");
    let (code, refused) = done(&app, "PIN-1", "s-1", "Done").await;
    assert_eq!(code, StatusCode::CONFLICT, "{refused}");
    assert!(
        refused["message"]
            .as_str()
            .unwrap()
            .contains("\"Watch or poll?\""),
        "{refused}"
    );
}

#[tokio::test]
async fn no_one_can_mark_done_an_item_with_open_or_in_progress_sub_items() {
    let app = app();
    for t in ["Parent", "Open part", "Going part", "Done part"] {
        add(&app, t).await;
    }
    for k in ["PIN-2", "PIN-3", "PIN-4"] {
        edit(&app, k, "s-1", json!({"parent": "PIN-1"})).await;
    }
    edit(&app, "PIN-3", "s-2", json!({"claim": "claim"})).await;
    done(&app, "PIN-4", "s-1", "Done").await;
    let (code, refused) = done(&app, "PIN-1", "s-1", "All done").await;
    assert_eq!(code, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused["message"],
        "PIN-1 is not done: its sub-items PIN-2, PIN-3 are still open"
    );
    // nor the person, from the done button or from the status menu
    let (code, refused) = call(&app, "POST", "/api/v1/items/PIN-1/done", Some(json!({}))).await;
    assert_eq!(code, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused["message"],
        "PIN-1 is not done: its sub-items PIN-2, PIN-3 are still open"
    );
    let (code, _) = call(
        &app,
        "PATCH",
        "/api/v1/items/PIN-1",
        Some(json!({"status": "done"})),
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT);
    assert_eq!(item(&app, "PIN-1").await["status"], "open");
    // once each is done or cancelled, it can be
    call(
        &app,
        "PATCH",
        "/api/v1/items/PIN-2",
        Some(json!({"status": "cancelled"})),
    )
    .await;
    call(&app, "POST", "/api/v1/items/PIN-3/done", Some(json!({}))).await;
    let (code, closed) = call(&app, "POST", "/api/v1/items/PIN-1/done", Some(json!({}))).await;
    assert_eq!(code, StatusCode::OK, "{closed}");
}

#[tokio::test]
async fn done_says_what_became_ready_and_the_parent_it_closed() {
    let app = app();
    for t in [
        "Parent",
        "Last part",
        "Waits for it",
        "Waits for more",
        "More",
    ] {
        add(&app, t).await;
    }
    edit(&app, "PIN-2", "s-1", json!({"parent": "PIN-1"})).await;
    edit(&app, "PIN-3", "s-1", json!({"depends_on": ["PIN-2"]})).await;
    edit(
        &app,
        "PIN-4",
        "s-1",
        json!({"depends_on": ["PIN-2", "PIN-5"]}),
    )
    .await;
    let (code, closed) = done(&app, "PIN-2", "s-1", "Done").await;
    assert_eq!(code, StatusCode::OK, "{closed}");
    assert_eq!(keys(&closed["now_ready"]), ["PIN-3"]);
    // its last sub-item done, the parent is done too
    assert_eq!(keys(&closed["parents_closed"]), ["PIN-1"]);
    assert_eq!(closed["parent_left_open"], Value::Null);
    assert_eq!(item(&app, "PIN-1").await["status"], "done");
}

#[tokio::test]
async fn an_agent_cancels_only_with_a_comment_and_a_dependent_stops_waiting() {
    let app = app();
    add(&app, "Duplicate").await;
    add(&app, "Waits for it").await;
    edit(&app, "PIN-2", "s-1", json!({"depends_on": ["PIN-1"]})).await;
    let (code, refused) = edit(&app, "PIN-1", "s-1", json!({"status": "cancelled"})).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(refused["violations"][0]["path"], "/comment");
    let (code, cancelled) = edit(
        &app,
        "PIN-1",
        "s-1",
        json!({"status": "cancelled", "comment": "A duplicate of PIN-7"}),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{cancelled}");
    assert_eq!(cancelled["item"]["status"], "cancelled");
    let waiting = item(&app, "PIN-2").await;
    assert_eq!(waiting["ready"], true);
    assert_eq!(waiting["depends_on"][0]["status"], "cancelled");
    // the person cancels without a comment
    add(&app, "Not needed").await;
    let (code, _) = call(
        &app,
        "PATCH",
        "/api/v1/items/PIN-3",
        Some(json!({"status": "cancelled"})),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
}

// --- changes and the timeline

#[tokio::test]
async fn an_edit_is_applied_whole_or_not_at_all() {
    let app = app();
    add(&app, "A").await;
    add(&app, "B").await;
    edit(&app, "PIN-2", "s-1", json!({"depends_on": ["PIN-1"]})).await;
    // a new title with a dependency that closes a cycle: neither is kept
    let (code, refused) = edit(
        &app,
        "PIN-1",
        "s-1",
        json!({"title": "Renamed", "depends_on": ["PIN-2"]}),
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused["message"],
        "this would make PIN-1 wait for itself: PIN-1 → PIN-2 → PIN-1"
    );
    assert_eq!(item(&app, "PIN-1").await["title"], "A");
}

#[tokio::test]
async fn each_change_goes_on_the_timeline_with_who_made_it() {
    let app = app();
    add(&app, "Item").await;
    add(&app, "Other").await;
    add_to(&app, "elsewhere", "There").await;
    edit(
        &app,
        "PIN-1",
        "s-1",
        json!({"title": "Item, renamed", "body": "More", "link": "https://example.com/1"}),
    )
    .await;
    edit(&app, "PIN-1", "s-1", json!({"depends_on": ["PIN-2"]})).await;
    edit(&app, "PIN-1", "s-1", json!({"no_depends_on": ["PIN-2"]})).await;
    edit(&app, "PIN-1", "s-1", json!({"parent": "PIN-2"})).await;
    let (_, moved) = edit(&app, "PIN-1", "s-1", json!({"repo": "elsewhere"})).await;
    assert_eq!(moved["item"]["key"], "ELS-2");
    let read = item(&app, "PIN-1").await;
    assert_eq!(read["key"], "ELS-2");
    assert_eq!(
        timeline_kinds(&read),
        ["created", "edited", "depends", "depends", "parent", "moved"]
    );
    let entries = read["timeline"].as_array().unwrap();
    assert_eq!(
        entries[1]["attrs"]["fields"],
        json!(["title", "body", "link"])
    );
    assert_eq!(entries[2]["attrs"]["added"], json!(["PIN-2"]));
    assert_eq!(entries[3]["attrs"]["removed"], json!(["PIN-2"]));
    assert_eq!(entries[5]["attrs"], json!({"from": "PIN-1", "to": "ELS-2"}));
    assert_eq!(entries[1]["actor"], "claude-code");
    assert_eq!(entries[1]["session"], "s-1");
}

// --- comments

#[tokio::test]
async fn the_person_and_agents_comment_on_an_item() {
    let app = app();
    add(&app, "Work").await;
    let (code, theirs) = call(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        Some(json!({"body": "Prefer a file watcher."})),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{theirs}");
    assert_eq!(theirs["author"], "person");
    let (code, mine) = agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        "s-1",
        json!({"body": "The watcher misses renames on Linux."}),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{mine}");
    assert_eq!(mine["author"], "claude-code");
    assert_eq!(mine["session"], "s-1");
    let read = item(&app, "PIN-1").await;
    let bodies: Vec<&str> = read["timeline"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "comment")
        .map(|e| e["body"].as_str().unwrap())
        .collect();
    assert_eq!(
        bodies,
        [
            "Prefer a file watcher.",
            "The watcher misses renames on Linux."
        ]
    );
    let (code, _) = agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        "s-1",
        json!({"body": "  "}),
    )
    .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_comment_is_edited_by_its_author_and_deleted_by_its_author_or_the_person() {
    let app = app();
    add(&app, "Work").await;
    let (_, mine) = agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        "s-1",
        json!({"body": "First"}),
    )
    .await;
    let id = mine["id"].as_str().unwrap();
    let path = format!("/api/v1/items/PIN-1/comments/{id}");
    let (code, edited) = agent(&app, "PATCH", &path, "s-1", json!({"body": "Second"})).await;
    assert_eq!(code, StatusCode::OK, "{edited}");
    assert_eq!(edited["body"], "Second");
    assert!(edited["edited_at"].is_string());
    // another agent can neither edit nor delete it
    let (code, _) = send(
        &app,
        "PATCH",
        &path,
        Some(json!({"body": "Third", "by": "codex", "session": "s-9"})),
    )
    .await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    let (code, _) = send(
        &app,
        "DELETE",
        &format!("{path}?by=codex&session=s-9"),
        None,
    )
    .await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    // the person can delete it
    let (code, _) = call(&app, "DELETE", &path, None).await;
    assert_eq!(code, StatusCode::NO_CONTENT);
    let (code, _) = call(&app, "DELETE", &path, None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_persons_comment_reaches_the_session_holding_the_item_and_no_other() {
    let app = app();
    add(&app, "Work").await;
    for session in ["s-1", "s-2"] {
        agent(
            &app,
            "POST",
            &format!("/api/v1/sessions/claude-code/{session}/status"),
            session,
            json!({"text": "Working"}),
        )
        .await;
    }
    edit(&app, "PIN-1", "s-1", json!({"claim": "claim"})).await;
    call(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        Some(json!({"body": "Prefer a file watcher."})),
    )
    .await;
    // an agent's own comment is not sent back to it
    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        "s-1",
        json!({"body": "Noted."}),
    )
    .await;
    let status = |session: &'static str| {
        let app = &app;
        async move {
            let (_, answer) = agent(
                app,
                "POST",
                &format!("/api/v1/sessions/claude-code/{session}/status"),
                session,
                json!({"text": "Still working"}),
            )
            .await;
            answer["messages"].clone()
        }
    };
    let delivered = status("s-1").await;
    assert_eq!(delivered.as_array().unwrap().len(), 1, "{delivered}");
    assert_eq!(
        delivered[0]["text"],
        "Comment on PIN-1 Work:\n\nPrefer a file watcher."
    );
    assert_eq!(status("s-2").await, json!([]));
}

#[tokio::test]
async fn a_comment_carries_files_that_the_sweep_keeps_until_the_comment_goes() {
    let app = app();
    add(&app, "Work").await;
    let bytes = b"error: the watcher stopped".to_vec();
    let hash = {
        use sha2::Digest;
        format!("{:x}", sha2::Sha256::digest(&bytes))
    };
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/attachments/{hash}"))
        .header("host", "127.0.0.1:4747")
        .header("content-type", "application/octet-stream")
        .header("content-length", bytes.len().to_string())
        .body(Body::from(bytes.clone()))
        .unwrap();
    assert!(
        app.router
            .clone()
            .oneshot(request)
            .await
            .unwrap()
            .status()
            .is_success()
    );
    let (code, comment) = agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        "s-1",
        json!({"body": "The log: [crash](attachment:crash.log)",
               "attachments": {"crash.log": {"sha256": hash, "size": bytes.len()}}}),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{comment}");
    assert_eq!(comment["files"][0]["name"], "crash.log");
    let (code, served) = call(&app, "GET", "/api/v1/items/PIN-1/files/crash.log", None).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(served, Value::String("error: the watcher stopped".into()));

    let later = chrono::Utc::now() + chrono::Duration::days(1);
    app.state.attachments().sweep(later).unwrap();
    let (code, _) = call(&app, "GET", "/api/v1/items/PIN-1/files/crash.log", None).await;
    assert_eq!(code, StatusCode::OK);

    let id = comment["id"].as_str().unwrap();
    call(
        &app,
        "DELETE",
        &format!("/api/v1/items/PIN-1/comments/{id}"),
        None,
    )
    .await;
    assert_eq!(app.state.attachments().sweep(later).unwrap(), 1);
    // a comment that shows a file it does not carry is refused
    let (code, _) = agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        "s-1",
        json!({"body": "See [the log](attachment:missing.log)"}),
    )
    .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
}

// --- reviews and sessions

fn item_keys(review: &Value) -> Vec<String> {
    keys(&review["items"])
}

#[tokio::test]
async fn a_review_from_the_session_holding_one_item_belongs_to_it() {
    let app = app();
    add(&app, "Work").await;
    add(&app, "Other").await;
    edit(&app, "PIN-1", "s-1", json!({"claim": "claim"})).await;
    let (code, sent) = submit(&app, review("s-1", "Theme loader", None)).await;
    assert_eq!(code, StatusCode::CREATED, "{sent}");
    assert_eq!(item_keys(&sent), ["PIN-1"]);
    assert_eq!(sent["items"][0]["title"], "Work");
    // named explicitly, by key or number, or an empty list for none
    let (_, named) = submit(&app, review("s-1", "Other one", Some(json!(["2"])))).await;
    assert_eq!(item_keys(&named), ["PIN-2"]);
    let (_, none) = submit(&app, review("s-1", "No item", Some(json!([])))).await;
    assert_eq!(none["items"], json!([]));
    let (code, refused) = submit(&app, review("s-1", "Unknown", Some(json!(["PIN-9"])))).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(refused["violations"][0]["path"], "/items/0");
    // another session, holding nothing, sends without an item
    let (_, other) = submit(&app, review("s-2", "Elsewhere", None)).await;
    assert_eq!(other["items"], json!([]));
    // a later round keeps its items
    let mut round = review("s-9", "Theme loader", None);
    round["revises"] = sent["id"].clone();
    let (_, round) = submit(&app, round).await;
    assert_eq!(item_keys(&round), ["PIN-1"]);

    let read = item(&app, "PIN-1").await;
    let titles: Vec<&str> = read["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Theme loader"]);
    assert_eq!(read["reviews"][0]["status"], "pending");
    assert!(timeline_kinds(&read).contains(&"review_sent".to_string()));
}

#[tokio::test]
async fn a_review_from_a_session_holding_two_items_belongs_to_both() {
    let app = app();
    add(&app, "One").await;
    add(&app, "Two").await;
    add(&app, "Three").await;
    edit(&app, "PIN-1", "s-1", json!({"claim": "claim"})).await;
    edit(&app, "PIN-2", "s-1", json!({"claim": "claim"})).await;
    let (code, sent) = submit(&app, review("s-1", "Both", None)).await;
    assert_eq!(code, StatusCode::CREATED, "{sent}");
    assert_eq!(item_keys(&sent), ["PIN-1", "PIN-2"]);
    for key in ["PIN-1", "PIN-2"] {
        let read = item(&app, key).await;
        assert_eq!(read["reviews"][0]["title"], "Both", "{key}");
        assert_eq!(read["waiting"], 1, "{key}");
        assert!(timeline_kinds(&read).contains(&"review_sent".to_string()));
    }
    // named items replace the default
    let (_, named) = submit(&app, review("s-1", "Only two", Some(json!(["PIN-2"])))).await;
    assert_eq!(item_keys(&named), ["PIN-2"]);
    let (_, named) = submit(&app, review("s-1", "Others", Some(json!(["3", "PIN-1"])))).await;
    assert_eq!(item_keys(&named), ["PIN-1", "PIN-3"]);
    // a named set on a new round applies to every round
    let mut round = review("s-1", "Both", Some(json!(["PIN-3"])));
    round["revises"] = sent["id"].clone();
    let (_, round) = submit(&app, round).await;
    assert_eq!(item_keys(&round), ["PIN-3"]);
    let (_, first) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews/{}", sent["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(item_keys(&first), ["PIN-3"]);
    assert!(
        item(&app, "PIN-1").await["reviews"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["title"] != "Both")
    );
}

#[tokio::test]
async fn a_dry_run_refuses_an_unknown_item() {
    let app = app();
    add(&app, "Work").await;
    let (code, ok) = call(
        &app,
        "POST",
        "/api/v1/reviews/validate",
        Some(review("s-1", "Fine", Some(json!(["PIN-1"])))),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{ok}");
    let (code, refused) = call(
        &app,
        "POST",
        "/api/v1/reviews/validate",
        Some(review("s-1", "Unknown", Some(json!(["PIN-1", "PIN-9"])))),
    )
    .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(refused["violations"][0]["path"], "/items/1");
}

#[tokio::test]
async fn an_agent_cannot_mark_done_either_item_of_a_waiting_review() {
    let app = app();
    add(&app, "One").await;
    add(&app, "Two").await;
    edit(&app, "PIN-1", "s-1", json!({"claim": "claim"})).await;
    edit(&app, "PIN-2", "s-1", json!({"claim": "claim"})).await;
    let (_, sent) = submit(&app, review("s-1", "Both", None)).await;
    for key in ["PIN-1", "PIN-2"] {
        let (code, refused) = done(&app, key, "s-1", "Shipped").await;
        assert_eq!(code, StatusCode::CONFLICT, "{key}: {refused}");
    }
    decide(&app, sent["id"].as_str().unwrap()).await;
    for key in ["PIN-1", "PIN-2"] {
        let (code, closed) = done(&app, key, "s-1", "Shipped").await;
        assert_eq!(code, StatusCode::OK, "{key}: {closed}");
    }
}

#[tokio::test]
async fn a_reviews_end_stays_on_the_items_timeline_after_the_history_sweep() {
    let app = app();
    add(&app, "Work").await;
    let (_, sent) = submit(&app, review("s-1", "Theme loader", Some(json!(["PIN-1"])))).await;
    let id = sent["id"].as_str().unwrap().to_string();
    decide(&app, &id).await;
    let ended = |read: &Value| {
        read["timeline"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "review_ended")
            .cloned()
            .unwrap()
    };
    let before = ended(&item(&app, "PIN-1").await);
    assert_eq!(before["attrs"]["review"], id);
    assert_eq!(before["attrs"]["title"], "Theme loader");
    assert_eq!(before["attrs"]["outcome"], "decided");

    let later = chrono::Utc::now() + chrono::Duration::days(1);
    app.state.reviews().sweep_history_before(later).unwrap();
    let (code, _) = call(&app, "GET", &format!("/api/v1/reviews/{id}"), None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let after = item(&app, "PIN-1").await;
    assert_eq!(ended(&after), before);
    assert!(after["reviews"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn deleting_an_item_drops_only_its_link() {
    let app = app();
    add(&app, "Work").await;
    add(&app, "Other").await;
    let (_, sent) = submit(
        &app,
        review("s-1", "Theme loader", Some(json!(["PIN-1", "PIN-2"]))),
    )
    .await;
    let (code, deleted) = call(&app, "DELETE", "/api/v1/items/PIN-1", None).await;
    assert_eq!(code, StatusCode::OK, "{deleted}");
    assert_eq!(deleted["deleted"], 1);
    let (_, read) = call(
        &app,
        "GET",
        &format!("/api/v1/reviews/{}", sent["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(item_keys(&read), ["PIN-2"]);
}

#[tokio::test]
async fn an_item_lists_the_sessions_that_worked_on_it_with_their_status() {
    let app = app();
    add(&app, "Work").await;
    agent(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-2/status",
        "s-2",
        json!({"text": "Running the theme tests"}),
    )
    .await;
    edit(&app, "PIN-1", "s-2", json!({"claim": "claim"})).await;
    let read = item(&app, "PIN-1").await;
    let sessions: Vec<(String, Value)> = read["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["session"].as_str().unwrap().into(), s["status"].clone()))
        .collect();
    assert!(
        sessions.contains(&("s-2".into(), json!("Running the theme tests"))),
        "{sessions:?}"
    );
    // the session that added it
    assert!(sessions.iter().any(|(s, _)| s == "s-1"), "{sessions:?}");
}

// --- the API's refusals and events

#[tokio::test]
async fn an_agent_cannot_delete_an_item() {
    let app = app();
    add(&app, "Work").await;
    let (code, refused) = send(&app, "DELETE", "/api/v1/items/PIN-1", None).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{refused}");
    assert_eq!(refused["error"], "forbidden");
    assert_eq!(item(&app, "PIN-1").await["status"], "open");
}

#[tokio::test]
async fn a_repositorys_prefix_is_changed_through_the_api() {
    let app = app();
    add(&app, "Work").await;
    let (code, _) = call(
        &app,
        "PATCH",
        "/api/v1/repos/forgeplane%2Fpinrail",
        Some(json!({"prefix": "PR"})),
    )
    .await;
    assert_eq!(code, StatusCode::NO_CONTENT);
    assert_eq!(item(&app, "PIN-1").await["key"], "PR-1");
    add_to(&app, "acme/pin", "Other").await;
    let (code, refused) = call(
        &app,
        "PATCH",
        "/api/v1/repos/acme%2Fpin",
        Some(json!({"prefix": "PIN"})),
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT, "{refused}");
    let (code, _) = call(
        &app,
        "PATCH",
        "/api/v1/repos/acme%2Fpin",
        Some(json!({"prefix": "pin"})),
    )
    .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    let (code, _) = call(
        &app,
        "PATCH",
        "/api/v1/repos/nowhere",
        Some(json!({"prefix": "NW"})),
    )
    .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn each_change_is_announced_and_a_delete_too() {
    let app = app();
    let start = app
        .state
        .events()
        .after(0, 10_000)
        .unwrap()
        .last()
        .map(|n| n.event_id)
        .unwrap_or(0);
    add(&app, "Work").await;
    edit(&app, "PIN-1", "s-1", json!({"title": "Renamed"})).await;
    call(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        Some(json!({"body": "Noted"})),
    )
    .await;
    call(&app, "DELETE", "/api/v1/items/PIN-1", None).await;
    let kinds: Vec<String> = app
        .state
        .events()
        .after(start, 100)
        .unwrap()
        .into_iter()
        .map(|n| n.kind)
        .collect();
    assert_eq!(
        kinds,
        [
            "item_changed",
            "item_changed",
            "item_changed",
            "item_deleted"
        ]
    );
}

#[tokio::test]
async fn a_review_that_expires_is_recorded_on_its_item() {
    let app = app();
    add(&app, "Work").await;
    let soon = chrono::Utc::now() + chrono::Duration::seconds(1);
    let mut body = review("s-1", "Short-lived", Some(json!(["PIN-1"])));
    body["expires_at"] = json!(soon.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    let (code, sent) = submit(&app, body).await;
    assert_eq!(code, StatusCode::CREATED, "{sent}");
    std::thread::sleep(std::time::Duration::from_millis(2100));
    app.state.reviews().sweep_expired().unwrap();
    let read = item(&app, "PIN-1").await;
    let ended = read["timeline"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "review_ended")
        .cloned()
        .unwrap();
    assert_eq!(ended["attrs"]["outcome"], "expired");
    assert_eq!(read["reviews"][0]["status"], "expired");
    // an expired review no longer stops an agent marking the item done
    let (code, closed) = done(&app, "PIN-1", "s-1", "Done without it").await;
    assert_eq!(code, StatusCode::OK, "{closed}");
}

#[tokio::test]
async fn an_items_body_carries_files_added_and_removed_by_name() {
    let app = app();
    let bytes = b"PNG pretend".to_vec();
    let hash = {
        use sha2::Digest;
        format!("{:x}", sha2::Sha256::digest(&bytes))
    };
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/attachments/{hash}"))
        .header("host", "127.0.0.1:4747")
        .header("content-type", "application/octet-stream")
        .header("content-length", bytes.len().to_string())
        .body(Body::from(bytes.clone()))
        .unwrap();
    app.router.clone().oneshot(request).await.unwrap();
    let file = json!({"dialog.png": {"sha256": hash, "size": bytes.len()}});
    let (code, added) = agent(
        &app,
        "POST",
        "/api/v1/items",
        "s-1",
        json!({"repo": REPO, "title": "The dialog", "body": "![The error](attachment:dialog.png)",
               "attachments": file}),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{added}");
    assert_eq!(added["files"][0]["name"], "dialog.png");
    // a body that shows a file it does not carry is refused
    let (code, _) = agent(
        &app,
        "POST",
        "/api/v1/items",
        "s-1",
        json!({"repo": REPO, "title": "Missing", "body": "![x](attachment:none.png)"}),
    )
    .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    let (code, edited) = edit(&app, "PIN-1", "s-1", json!({"detach": ["dialog.png"]})).await;
    assert_eq!(code, StatusCode::OK, "{edited}");
    assert!(edited["item"]["files"].as_array().unwrap().is_empty());
    let (code, _) = call(&app, "GET", "/api/v1/items/PIN-1/files/dialog.png", None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

/// Stores `bytes` in the blob store, and answers their hash.
async fn upload(app: &App, bytes: &[u8]) -> String {
    let hash = {
        use sha2::Digest;
        format!("{:x}", sha2::Sha256::digest(bytes))
    };
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/attachments/{hash}"))
        .header("host", "127.0.0.1:4747")
        .header("content-type", "application/octet-stream")
        .header("content-length", bytes.len().to_string())
        .body(Body::from(bytes.to_vec()))
        .unwrap();
    let status = app.router.clone().oneshot(request).await.unwrap().status();
    assert!(status.is_success(), "{status}");
    hash
}

#[tokio::test]
async fn a_file_keeps_its_name_against_a_file_of_another_comment_or_the_description() {
    let app = app();
    let first = upload(&app, b"the first picture").await;
    let second = upload(&app, b"the second picture").await;
    let file = |hash: &str, size: usize| json!({"image.png": {"sha256": hash, "size": size}});
    let (code, added) = agent(
        &app,
        "POST",
        "/api/v1/items",
        "s-1",
        json!({"repo": REPO, "title": "The dialog", "body": "![before](attachment:image.png)",
               "attachments": file(&first, 17)}),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{added}");

    // a comment's file under the description's file's name
    let (code, answer) = agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        "s-1",
        json!({"body": "![after](attachment:image.png)", "attachments": file(&second, 18)}),
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT, "{answer}");
    assert_eq!(
        answer["message"],
        "PIN-1 already has a file named image.png, in its description: give this one another name"
    );
    let (_, served) = call(&app, "GET", "/api/v1/items/PIN-1/files/image.png", None).await;
    assert_eq!(served, Value::String("the first picture".into()));

    // a comment's file under another comment's file's name
    let other = json!({"log.txt": {"sha256": second, "size": 18}});
    let (code, _) = agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        "s-1",
        json!({"body": "[log](attachment:log.txt)", "attachments": other}),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED);
    let (code, answer) = agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        "s-1",
        json!({"body": "[log](attachment:log.txt)",
               "attachments": {"log.txt": {"sha256": first, "size": 17}}}),
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT, "{answer}");
    assert_eq!(
        answer["message"],
        "PIN-1 already has a file named log.txt, in a comment: give this one another name"
    );

    // the description's own file is replaced by its new version
    let (code, edited) = edit(
        &app,
        "PIN-1",
        "s-1",
        json!({"attachments": file(&second, 18)}),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{edited}");
    let (_, served) = call(&app, "GET", "/api/v1/items/PIN-1/files/image.png", None).await;
    assert_eq!(served, Value::String("the second picture".into()));
}

#[tokio::test]
async fn the_review_list_narrows_to_a_work_items_reviews() {
    let app = app();
    add(&app, "Work").await;
    add(&app, "Other").await;
    submit(&app, review("s-1", "For the work", Some(json!(["PIN-1"])))).await;
    submit(&app, review("s-1", "For the other", Some(json!(["PIN-2"])))).await;
    submit(&app, review("s-1", "For nothing", Some(json!([])))).await;
    let (code, listed) = call(&app, "GET", "/api/v1/reviews?item=PIN-1", None).await;
    assert_eq!(code, StatusCode::OK, "{listed}");
    let titles: Vec<&str> = listed["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["For the work"]);
    let (code, _) = call(&app, "GET", "/api/v1/reviews?item=PIN-9", None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_item_says_how_much_of_it_waits_and_what_project_it_is_in() {
    let app = app();
    add(&app, "Work").await;
    assert_eq!(item(&app, "PIN-1").await["waiting"], 0);
    assert_eq!(item(&app, "PIN-1").await["project"], Value::Null);
    let (_, first) = submit(&app, review("s-1", "One", Some(json!(["PIN-1"])))).await;
    submit(&app, review("s-1", "Two", Some(json!(["PIN-1"])))).await;
    // a newer round counts once
    let mut round = review("s-1", "One", None);
    round["revises"] = first["id"].clone();
    submit(&app, round).await;
    assert_eq!(item(&app, "PIN-1").await["waiting"], 2);
    let (_, listed) = call(&app, "GET", "/api/v1/items", None).await;
    assert_eq!(listed["items"][0]["waiting"], 2);
    let (_, project) = call(
        &app,
        "POST",
        "/api/v1/projects",
        Some(json!({"name": "Pinrail", "repos": [REPO]})),
    )
    .await;
    let read = item(&app, "PIN-1").await;
    assert_eq!(read["project"]["id"], project["id"]);
    assert_eq!(read["project"]["name"], "Pinrail");
}

#[tokio::test]
async fn cancelling_an_item_cancels_its_open_sub_items_at_any_depth() {
    let app = app();
    for t in [
        "Parent",
        "Open part",
        "Held part",
        "Done part",
        "Grandchild",
    ] {
        add(&app, t).await;
    }
    for k in ["PIN-2", "PIN-3", "PIN-4"] {
        edit(&app, k, "s-1", json!({"parent": "PIN-1"})).await;
    }
    edit(&app, "PIN-5", "s-1", json!({"parent": "PIN-2"})).await;
    edit(&app, "PIN-3", "s-2", json!({"claim": "claim"})).await;
    done(&app, "PIN-4", "s-1", "Finished").await;

    let (code, cancelled) = call(
        &app,
        "PATCH",
        "/api/v1/items/PIN-1",
        Some(json!({"status": "cancelled"})),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{cancelled}");
    assert_eq!(
        keys(&cancelled["cancelled_with"]),
        ["PIN-2", "PIN-3", "PIN-5"]
    );
    for k in ["PIN-2", "PIN-3", "PIN-5"] {
        let read = item(&app, k).await;
        assert_eq!(read["status"], "cancelled", "{k}");
        assert_eq!(read["claim"], Value::Null, "{k}");
        let last = read["timeline"].as_array().unwrap().last().unwrap().clone();
        assert_eq!(last["kind"], "status", "{k}");
        assert_eq!(last["attrs"]["with"], "PIN-1", "{k}");
    }
    // what was done stays done
    assert_eq!(item(&app, "PIN-4").await["status"], "done");
    // an agent cancels its sub-items with it too, with its reason
    add(&app, "Another").await;
    add(&app, "Its part").await;
    edit(&app, "PIN-7", "s-1", json!({"parent": "PIN-6"})).await;
    let (code, _) = edit(
        &app,
        "PIN-6",
        "s-1",
        json!({"status": "cancelled", "comment": "A duplicate"}),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(item(&app, "PIN-7").await["status"], "cancelled");
}

#[tokio::test]
async fn a_reviews_items_are_set_and_changed_on_every_round() {
    let app = app();
    add(&app, "First").await;
    add(&app, "Second").await;
    add(&app, "Third").await;
    let (_, one) = submit(&app, review("s-1", "Theme loader", Some(json!(["PIN-1"])))).await;
    let mut next = review("s-1", "Theme loader", None);
    next["revises"] = one["id"].clone();
    let (_, two) = submit(&app, next).await;
    let start = app
        .state
        .events()
        .after(0, 10_000)
        .unwrap()
        .last()
        .map(|n| n.event_id)
        .unwrap_or(0);
    let path = format!("/api/v1/reviews/{}/items", two["id"].as_str().unwrap());
    let (code, _) = call(
        &app,
        "PUT",
        &path,
        Some(json!({"items": ["PIN-2", "PIN-3"]})),
    )
    .await;
    assert_eq!(code, StatusCode::NO_CONTENT);
    for id in [&one["id"], &two["id"]] {
        let (_, read) = call(
            &app,
            "GET",
            &format!("/api/v1/reviews/{}", id.as_str().unwrap()),
            None,
        )
        .await;
        assert_eq!(item_keys(&read), ["PIN-2", "PIN-3"], "{id}");
    }
    // each item it joined and left records it, and is announced
    let first = item(&app, "PIN-1").await;
    assert!(first["reviews"].as_array().unwrap().is_empty());
    assert_eq!(timeline_kinds(&first).last().unwrap(), "review_removed");
    for key in ["PIN-2", "PIN-3"] {
        let read = item(&app, key).await;
        assert_eq!(read["reviews"][0]["title"], "Theme loader");
        assert_eq!(timeline_kinds(&read).last().unwrap(), "review_added");
    }
    let announced = app
        .state
        .events()
        .after(start, 100)
        .unwrap()
        .into_iter()
        .filter(|n| n.kind == "item_changed")
        .count();
    assert_eq!(announced, 3);
    // and to none
    let (code, _) = call(&app, "PUT", &path, Some(json!({"items": []}))).await;
    assert_eq!(code, StatusCode::NO_CONTENT);
    assert!(
        item(&app, "PIN-2").await["reviews"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // an unknown item or review is not found
    let (code, _) = call(&app, "PUT", &path, Some(json!({"items": ["PIN-9"]}))).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let (code, _) = call(
        &app,
        "PUT",
        "/api/v1/reviews/r_none/items",
        Some(json!({"items": ["PIN-1"]})),
    )
    .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_list_narrows_to_the_items_a_session_holds_or_worked_on() {
    let app = app();
    for t in [
        "Added by s-1",
        "Held by s-2",
        "Commented by s-2",
        "Reviewed by s-2",
        "Untouched",
    ] {
        add(&app, t).await;
    }
    edit(&app, "PIN-2", "s-2", json!({"claim": "claim"})).await;
    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-3/comments",
        "s-2",
        json!({"body": "Noted"}),
    )
    .await;
    submit(&app, review("s-2", "For it", Some(json!(["PIN-4"])))).await;
    let (_, listed) = call(
        &app,
        "GET",
        "/api/v1/items?agent=claude-code&session=s-2",
        None,
    )
    .await;
    assert_eq!(keys(&listed["items"]), ["PIN-2", "PIN-3", "PIN-4"]);
    let (_, listed) = call(
        &app,
        "GET",
        "/api/v1/items?agent=claude-code&session=s-1",
        None,
    )
    .await;
    assert_eq!(
        keys(&listed["items"]),
        ["PIN-1", "PIN-2", "PIN-3", "PIN-4", "PIN-5"]
    );
}

#[tokio::test]
async fn the_projects_listing_gives_each_repository_its_prefix_and_shows_one_with_items_alone() {
    let app = app();
    add(&app, "Work").await;
    let (_, listing) = call(&app, "GET", "/api/v1/projects", None).await;
    let repo = listing["loose"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["repo"] == REPO)
        .cloned()
        .unwrap();
    assert_eq!(repo["prefix"], "PIN");
}

// --- a session's comments

#[tokio::test]
async fn a_sessions_thread_lists_the_comments_its_agent_wrote_on_items() {
    let app = app();
    add(&app, "Work").await;
    add(&app, "Other").await;
    for session in ["s-1", "s-2"] {
        agent(
            &app,
            "POST",
            &format!("/api/v1/sessions/claude-code/{session}/status"),
            session,
            json!({"text": "Working"}),
        )
        .await;
    }
    let comment = |key: &'static str, session: &'static str, body: &'static str| {
        let app = &app;
        async move {
            let (code, answer) = agent(
                app,
                "POST",
                &format!("/api/v1/items/{key}/comments"),
                session,
                json!({"body": body}),
            )
            .await;
            assert_eq!(code, StatusCode::CREATED, "{answer}");
        }
    };
    comment("PIN-1", "s-1", "Found the **cause**.").await;
    comment("PIN-2", "s-1", "Not this one.").await;
    comment("PIN-1", "s-2", "From another session.").await;
    // the person's comments are not the session's
    call(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        Some(json!({"body": "Prefer a watcher."})),
    )
    .await;

    let (code, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
    assert_eq!(code, StatusCode::OK, "{thread}");
    let comments = thread["comments"].as_array().unwrap();
    let said: Vec<(&str, &str, &str)> = comments
        .iter()
        .map(|c| {
            (
                c["body"].as_str().unwrap(),
                c["item"]["key"].as_str().unwrap(),
                c["item"]["title"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            ("Found the **cause**.", "PIN-1", "Work"),
            ("Not this one.", "PIN-2", "Other"),
        ]
    );
    assert!(comments[0]["created_at"].is_string());
    assert_eq!(comments[0]["files"], json!([]));

    // a deleted comment, or one of a deleted item, leaves the thread
    let id = comments[1]["id"].as_str().unwrap().to_string();
    let (code, _) = send(
        &app,
        "DELETE",
        &format!("/api/v1/items/PIN-2/comments/{id}?by=claude-code&session=s-1"),
        None,
    )
    .await;
    assert!(code.is_success(), "{code}");
    call(&app, "DELETE", "/api/v1/items/PIN-1", None).await;
    let (_, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
    assert_eq!(thread["comments"], json!([]));
}

#[tokio::test]
async fn times_order_a_thread_written_within_one_second() {
    let app = app();
    add(&app, "Work").await;
    let report = |text: &'static str| {
        let app = &app;
        async move {
            agent(
                app,
                "POST",
                "/api/v1/sessions/claude-code/s-1/status",
                "s-1",
                json!({"text": text}),
            )
            .await;
        }
    };
    report("Reading").await;
    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        "s-1",
        json!({"body": "Found it."}),
    )
    .await;
    report("Testing").await;
    let (_, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
    let at = |v: &Value| v.as_str().unwrap().to_string();
    let reading = at(&thread["statuses"][0]["at"]);
    let comment = at(&thread["comments"][0]["created_at"]);
    let testing = at(&thread["statuses"][1]["at"]);
    // to the microsecond, as text that sorts as the times do
    for time in [&reading, &comment, &testing] {
        assert_eq!(time.len(), "2026-11-07T10:00:00.000000Z".len(), "{time}");
    }
    assert!(
        reading < comment && comment < testing,
        "{reading} {comment} {testing}"
    );
}

#[tokio::test]
async fn a_sessions_thread_lists_what_its_agent_did_to_work_items() {
    let app = app();
    agent(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        "s-1",
        json!({"text": "Working"}),
    )
    .await;
    // added by this session, claimed, released, claimed again and done
    add(&app, "Work").await;
    edit(&app, "PIN-1", "s-1", json!({"claim": "claim"})).await;
    edit(&app, "PIN-1", "s-1", json!({"claim": "release"})).await;
    edit(&app, "PIN-1", "s-1", json!({"claim": "claim"})).await;
    let (code, answer) = done(&app, "PIN-1", "s-1", "Shipped").await;
    assert_eq!(code, StatusCode::OK, "{answer}");
    // a parent cancelled with comment, and the sub-item it takes with it
    add(&app, "Parent").await;
    let (code, answer) = agent(
        &app,
        "POST",
        "/api/v1/items",
        "s-1",
        json!({"repo": REPO, "title": "Child", "parent": "PIN-2"}),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{answer}");
    edit(
        &app,
        "PIN-2",
        "s-1",
        json!({"status": "cancelled", "comment": "Not needed"}),
    )
    .await;
    // another session's, and the person's, are not this session's
    edit(&app, "PIN-1", "s-2", json!({"status": "open"})).await;
    call(
        &app,
        "PATCH",
        "/api/v1/items/PIN-3",
        Some(json!({"title": "Renamed"})),
    )
    .await;

    let (_, thread) = call(&app, "GET", "/api/v1/sessions/claude-code/s-1", None).await;
    let did: Vec<String> = thread["item_events"]
        .as_array()
        .unwrap_or_else(|| panic!("no item_events in {thread}"))
        .iter()
        .map(|e| {
            format!(
                "{} {} {}",
                e["kind"].as_str().unwrap(),
                e["item"]["key"].as_str().unwrap(),
                e["attrs"]["to"].as_str().unwrap_or("-")
            )
        })
        .collect();
    assert_eq!(
        did,
        [
            "created PIN-1 -",
            "claimed PIN-1 -",
            "released PIN-1 -",
            "claimed PIN-1 -",
            "status PIN-1 done",
            "created PIN-2 -",
            "created PIN-3 -",
            "status PIN-2 cancelled",
        ]
    );
    let first = &thread["item_events"][0];
    assert_eq!(first["item"]["title"], "Work");
    assert!(first["at"].is_string());
}

#[tokio::test]
async fn an_attachment_link_in_code_is_text_and_a_missing_file_is_named_as_the_items() {
    let app = app();
    let body = "Show an image with `![p95](attachment:latency.png)`:\n\n\
                ```md\n![p95](attachment:latency.png)\n```\n\n\
                ~~~\n[log](attachment:run.log)\n~~~\n";
    let (code, added) = call(
        &app,
        "POST",
        "/api/v1/items",
        Some(json!({"repo": REPO, "title": "Mentions", "body": body})),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{added}");

    let (code, refused) = call(
        &app,
        "POST",
        "/api/v1/items",
        Some(
            json!({"repo": REPO, "title": "Shows", "body": "See ![p95](attachment:latency.png)."}),
        ),
    )
    .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    let said = refused["violations"][0]["message"].as_str().unwrap();
    assert!(
        said.contains("which the description does not carry"),
        "{said}"
    );

    let (code, refused) = call(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        Some(json!({"body": "See [log](attachment:run.log)."})),
    )
    .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    let said = refused["violations"][0]["message"].as_str().unwrap();
    assert!(said.contains("which the comment does not carry"), "{said}");
}

// --- on hold

async fn hold(app: &App, key: &str, status: &str) -> (StatusCode, Value) {
    call(
        app,
        "PATCH",
        &format!("/api/v1/items/{key}"),
        Some(json!({"status": status})),
    )
    .await
}

#[tokio::test]
async fn next_skips_an_item_on_hold_and_its_sub_items() {
    let app = app();
    add(&app, "Held").await;
    add(&app, "Parent held").await;
    let (code, sub) = call(
        &app,
        "POST",
        "/api/v1/items",
        Some(json!({"repo": REPO, "title": "Sub-item", "parent": "PIN-2"})),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{sub}");
    add(&app, "Free").await;
    let (code, held) = hold(&app, "PIN-1", "on_hold").await;
    assert_eq!(code, StatusCode::OK, "{held}");
    assert_eq!(held["item"]["status"], "on_hold");
    assert_eq!(held["item"]["ready"], false);
    hold(&app, "PIN-2", "on_hold").await;
    assert_eq!(next(&app, "s-1", false).await["key"], "PIN-4");
    // listed on its own, and not with the open items
    let (_, open) = call(&app, "GET", "/api/v1/items?status=open", None).await;
    assert_eq!(keys(&open["items"]), ["PIN-3", "PIN-4"]);
    let (_, on_hold) = call(&app, "GET", "/api/v1/items?status=on_hold", None).await;
    assert_eq!(keys(&on_hold["items"]), ["PIN-1", "PIN-2"]);
    // taken off hold, it is ready again
    let (_, open) = hold(&app, "PIN-1", "open").await;
    assert_eq!(open["item"]["status"], "open");
    assert_eq!(open["item"]["ready"], true);
    assert!(timeline_kinds(&open["item"]).contains(&"status".to_string()));
}

#[tokio::test]
async fn an_items_sub_items_list_includes_those_on_hold() {
    let app = app();
    add(&app, "Parent").await;
    for title in ["Held", "Open"] {
        let (code, sub) = call(
            &app,
            "POST",
            "/api/v1/items",
            Some(json!({"repo": REPO, "title": title, "parent": "PIN-1"})),
        )
        .await;
        assert_eq!(code, StatusCode::CREATED, "{sub}");
    }
    hold(&app, "PIN-2", "on_hold").await;
    let parent = item(&app, "PIN-1").await;
    assert_eq!(parent["sub_items"]["total"], 2);
    assert_eq!(keys(&parent["sub_items_list"]), ["PIN-2", "PIN-3"]);
}

#[tokio::test]
async fn only_the_person_puts_an_item_on_hold_or_takes_it_off() {
    let app = app();
    add(&app, "Held").await;
    add(&app, "Free").await;
    let (code, refused) = edit(&app, "PIN-2", "s-1", json!({"status": "on_hold"})).await;
    assert_eq!(code, StatusCode::CONFLICT, "{refused}");
    hold(&app, "PIN-1", "on_hold").await;
    let (code, refused) = edit(&app, "PIN-1", "s-1", json!({"claim": "claim"})).await;
    assert_eq!(code, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused["message"],
        "PIN-1 is on hold: ask the person to take it off hold"
    );
    let (code, _) = edit(&app, "PIN-1", "s-1", json!({"status": "open"})).await;
    assert_eq!(code, StatusCode::CONFLICT);
    assert_eq!(item(&app, "PIN-1").await["status"], "on_hold");
}

#[tokio::test]
async fn an_item_on_hold_still_blocks_what_depends_on_it() {
    let app = app();
    add(&app, "Held").await;
    let (code, _) = call(
        &app,
        "POST",
        "/api/v1/items",
        Some(json!({"repo": REPO, "title": "Waits", "depends_on": ["PIN-1"]})),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED);
    hold(&app, "PIN-1", "on_hold").await;
    let waits = item(&app, "PIN-2").await;
    assert_eq!(waits["ready"], false);
    assert_eq!(waits["blocked_by"], json!(["PIN-1"]));
}

#[tokio::test]
async fn putting_an_item_in_progress_on_hold_releases_it_and_tells_the_session() {
    let app = app();
    add(&app, "Work").await;
    agent(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        "s-1",
        json!({"text": "Working"}),
    )
    .await;
    edit(&app, "PIN-1", "s-1", json!({"claim": "claim"})).await;
    let (code, held) = hold(&app, "PIN-1", "on_hold").await;
    assert_eq!(code, StatusCode::OK, "{held}");
    assert_eq!(held["item"]["status"], "on_hold");
    assert_eq!(held["item"]["claim"], Value::Null);
    let (_, answer) = agent(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        "s-1",
        json!({"text": "Still working"}),
    )
    .await;
    assert_eq!(
        answer["messages"][0]["text"],
        "PIN-1 Work is on hold now: stop working on it, and take another item."
    );
}

#[tokio::test]
async fn putting_a_parent_on_hold_releases_its_sub_items_and_tells_their_sessions() {
    let app = app();
    add(&app, "Release").await;
    add(&app, "Notes").await;
    add(&app, "Draft").await;
    edit(&app, "PIN-2", "s-1", json!({"parent": "PIN-1"})).await;
    edit(&app, "PIN-3", "s-1", json!({"parent": "PIN-2"})).await;
    agent(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-2/status",
        "s-2",
        json!({"text": "Working"}),
    )
    .await;
    edit(&app, "PIN-3", "s-2", json!({"claim": "claim"})).await;
    let (code, held) = hold(&app, "PIN-1", "on_hold").await;
    assert_eq!(code, StatusCode::OK, "{held}");
    // the sub-item two levels down is no longer in progress
    let sub = item(&app, "PIN-3").await;
    assert_eq!(sub["status"], "open");
    assert_eq!(sub["claim"], Value::Null);
    let (_, answer) = agent(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-2/status",
        "s-2",
        json!({"text": "Still working"}),
    )
    .await;
    assert_eq!(
        answer["messages"][0]["text"],
        "PIN-3 Draft is under PIN-1 Release, which is on hold now: stop working on it, and take another item."
    );
}

#[tokio::test]
async fn the_summary_list_leaves_the_bodies_out() {
    let app = app();
    let (code, _) = agent(
        &app,
        "POST",
        "/api/v1/items",
        "s-1",
        json!({"repo": REPO, "title": "With a body", "body": "A long description of the quokka"}),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED);
    let (_, full) = call(&app, "GET", "/api/v1/items", None).await;
    assert_eq!(full["items"][0]["body"], "A long description of the quokka");
    let (code, summary) = call(&app, "GET", "/api/v1/items?fields=summary", None).await;
    assert_eq!(code, StatusCode::OK, "{summary}");
    let item = &summary["items"][0];
    assert!(item.get("body").is_none(), "{item}");
    // the rest stays
    let mut without = full["items"][0].clone();
    without.as_object_mut().unwrap().remove("body");
    assert_eq!(item, &without);
    // and the words of a body still find it
    let (_, found) = call(&app, "GET", "/api/v1/items?fields=summary&q=quokka", None).await;
    assert_eq!(keys(&found["items"]), ["PIN-1"]);
}

#[tokio::test]
async fn a_parent_closes_with_its_last_sub_item_unless_it_is_worked_on_or_the_setting_is_off() {
    let app = app();
    add(&app, "Release").await;
    add(&app, "Notes").await;
    add(&app, "Tag").await;
    edit(&app, "PIN-2", "s-1", json!({"parent": "PIN-1"})).await;
    edit(&app, "PIN-3", "s-1", json!({"parent": "PIN-1"})).await;
    // a sub-item in progress shows on the parent
    edit(&app, "PIN-2", "s-1", json!({"claim": "claim"})).await;
    assert_eq!(item(&app, "PIN-1").await["sub_items"]["in_progress"], 1);

    done(&app, "PIN-2", "s-1", "Written").await;
    assert_eq!(item(&app, "PIN-1").await["status"], "open");
    let (code, answer) = done(&app, "PIN-3", "s-1", "Tagged").await;
    assert_eq!(code, StatusCode::OK, "{answer}");
    let parent = item(&app, "PIN-1").await;
    assert_eq!(parent["status"], "done");
    let last = parent["timeline"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["attrs"]["with_last"], "PIN-3", "{last}");

    // a parent a session works on stays open, as it does with the setting off
    add(&app, "Second").await;
    add(&app, "Under it").await;
    edit(&app, "PIN-5", "s-1", json!({"parent": "PIN-4"})).await;
    edit(&app, "PIN-4", "s-2", json!({"claim": "claim"})).await;
    let (_, answer) = done(&app, "PIN-5", "s-1", "Done").await;
    assert_eq!(item(&app, "PIN-4").await["status"], "in_progress");
    assert_eq!(answer["parent_left_open"]["key"], "PIN-4");

    let (code, _) = call(
        &app,
        "PATCH",
        "/api/v1/settings",
        Some(json!({"items": {"close_parents": false}})),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    add(&app, "Third").await;
    add(&app, "Under the third").await;
    edit(&app, "PIN-7", "s-1", json!({"parent": "PIN-6"})).await;
    done(&app, "PIN-7", "s-1", "Done").await;
    assert_eq!(item(&app, "PIN-6").await["status"], "open");
}

// --- the queue

async fn queue(app: &App, key: &str, place: Value) -> (StatusCode, Value) {
    call(
        app,
        "PUT",
        &format!("/api/v1/items/{key}/queue"),
        Some(place),
    )
    .await
}

async fn queued(app: &App) -> Vec<String> {
    let (code, list) = call(app, "GET", "/api/v1/items?queued=true", None).await;
    assert_eq!(code, StatusCode::OK, "{list}");
    keys(&list["items"])
}

#[tokio::test]
async fn the_queue_is_ordered_by_the_person_and_next_takes_it_first() {
    let app = app();
    for t in ["One", "Two", "Three", "Four"] {
        add(&app, t).await;
    }
    assert_eq!(next(&app, "s-1", false).await["key"], "PIN-1");
    let (code, answer) = queue(&app, "PIN-3", json!({})).await;
    assert_eq!(code, StatusCode::OK, "{answer}");
    queue(&app, "PIN-2", json!({})).await;
    queue(&app, "PIN-4", json!({"before": "PIN-3"})).await;
    assert_eq!(queued(&app).await, ["PIN-4", "PIN-3", "PIN-2"]);
    assert_eq!(item(&app, "PIN-3").await["queue"], 2);
    assert_eq!(item(&app, "PIN-1").await["queue"], Value::Null);
    assert_eq!(next(&app, "s-1", false).await["key"], "PIN-4");
    // moved after another
    queue(&app, "PIN-4", json!({"after": "PIN-2"})).await;
    assert_eq!(queued(&app).await, ["PIN-3", "PIN-2", "PIN-4"]);
    assert_eq!(next(&app, "s-1", false).await["key"], "PIN-3");
    // taken out
    let (code, _) = call(&app, "DELETE", "/api/v1/items/PIN-3/queue", None).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(queued(&app).await, ["PIN-2", "PIN-4"]);
    assert!(timeline_kinds(&item(&app, "PIN-3").await).contains(&"unqueued".to_string()));
    assert!(timeline_kinds(&item(&app, "PIN-2").await).contains(&"queued".to_string()));
}

#[tokio::test]
async fn a_move_next_to_an_item_not_in_the_queue_or_to_itself_is_refused() {
    let app = app();
    for t in ["One", "Two", "Three", "Four"] {
        add(&app, t).await;
    }
    queue(&app, "PIN-1", json!({})).await;
    queue(&app, "PIN-2", json!({})).await;
    queue(&app, "PIN-3", json!({"before": "PIN-2"})).await;
    let before = [
        item(&app, "PIN-1").await["queue"].clone(),
        item(&app, "PIN-3").await["queue"].clone(),
    ];

    // next to an item that is not queued
    let (code, answer) = queue(&app, "PIN-1", json!({"before": "PIN-4"})).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    assert_eq!(
        answer["violations"][0]["message"],
        "PIN-4 is not in the queue"
    );
    // next to itself
    let (code, answer) = queue(&app, "PIN-2", json!({"after": "PIN-2"})).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    assert_eq!(
        answer["violations"][0]["message"],
        "PIN-2 cannot be placed next to itself"
    );

    // nothing moved, nor was numbered again
    assert_eq!(queued(&app).await, ["PIN-1", "PIN-3", "PIN-2"]);
    assert_eq!(
        [
            item(&app, "PIN-1").await["queue"].clone(),
            item(&app, "PIN-3").await["queue"].clone()
        ],
        before
    );
    assert_eq!(item(&app, "PIN-4").await["queue"], Value::Null);
}

#[tokio::test]
async fn next_offers_the_sessions_sub_items_before_the_queue_and_passes_over_what_is_not_ready() {
    let app = app();
    add(&app, "Held by the session").await;
    let sub = |title: &'static str, parent: &'static str| json!({"repo": REPO, "title": title, "parent": parent});
    call(
        &app,
        "POST",
        "/api/v1/items",
        Some(sub("Its sub-item", "PIN-1")),
    )
    .await;
    add(&app, "Queued, blocked").await;
    add(&app, "Blocker").await;
    edit(&app, "PIN-3", "s-x", json!({"depends_on": ["PIN-4"]})).await;
    add(&app, "Queued parent").await;
    call(
        &app,
        "POST",
        "/api/v1/items",
        Some(sub("Under the queued parent", "PIN-5")),
    )
    .await;
    queue(&app, "PIN-3", json!({})).await;
    queue(&app, "PIN-5", json!({})).await;
    edit(&app, "PIN-1", "s-1", json!({"claim": "claim"})).await;
    // the session's own breakdown first
    assert_eq!(next(&app, "s-1", false).await["key"], "PIN-2");
    // another session: the blocked item is passed over, and the queued
    // parent offers its sub-item in its place
    assert_eq!(next(&app, "s-2", false).await["key"], "PIN-6");
}

#[tokio::test]
async fn an_item_leaves_the_queue_when_it_closes_or_waits_but_not_when_claimed() {
    let app = app();
    for t in ["Claimed", "Done", "Cancelled", "Held", "Deleted"] {
        add(&app, t).await;
    }
    for key in ["PIN-1", "PIN-2", "PIN-3", "PIN-4", "PIN-5"] {
        queue(&app, key, json!({})).await;
    }
    edit(&app, "PIN-1", "s-1", json!({"claim": "claim"})).await;
    edit(&app, "PIN-1", "s-1", json!({"claim": "release"})).await;
    edit(&app, "PIN-1", "s-1", json!({"claim": "claim"})).await;
    call(
        &app,
        "PATCH",
        "/api/v1/items/PIN-2",
        Some(json!({"status": "done"})),
    )
    .await;
    call(
        &app,
        "PATCH",
        "/api/v1/items/PIN-3",
        Some(json!({"status": "cancelled"})),
    )
    .await;
    call(
        &app,
        "PATCH",
        "/api/v1/items/PIN-4",
        Some(json!({"status": "on_hold"})),
    )
    .await;
    call(&app, "DELETE", "/api/v1/items/PIN-5", None).await;
    assert_eq!(queued(&app).await, ["PIN-1"]);
    // queuing an item on hold takes it off hold
    queue(&app, "PIN-4", json!({})).await;
    assert_eq!(item(&app, "PIN-4").await["status"], "open");
    assert_eq!(queued(&app).await, ["PIN-1", "PIN-4"]);
    // a closed item is not queued
    let (code, refused) = queue(&app, "PIN-2", json!({})).await;
    assert_eq!(code, StatusCode::CONFLICT, "{refused}");
}

#[tokio::test]
async fn only_the_person_changes_the_queue() {
    let app = app();
    add(&app, "Work").await;
    let (code, refused) = agent(&app, "PUT", "/api/v1/items/PIN-1/queue", "s-1", json!({})).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{refused}");
    assert_eq!(refused["message"], "only the person changes the queue");
    queue(&app, "PIN-1", json!({})).await;
    let (code, _) = agent(
        &app,
        "DELETE",
        "/api/v1/items/PIN-1/queue",
        "s-1",
        json!({}),
    )
    .await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    assert_eq!(queued(&app).await, ["PIN-1"]);
}
