//! `/api/v1/notifications`: the lists and their pages, the counts, and the
//! person's changes to the read and done state.

use axum::http::StatusCode;
use serde_json::{Value, json};

mod common;
use common::{App, app, call};

const REPO: &str = "forgeplane/pinrail";

async fn agent(app: &App, method: &str, path: &str, body: Value) -> Value {
    let mut body = body;
    body["by"] = json!("claude-code");
    body["session"] = json!("s-1");
    let (code, answer) = common::call_as(app, method, path, Some(body), &[]).await;
    assert!(code.is_success(), "{method} {path}: {code} {answer}");
    answer
}

/// Three items an agent added, each with its notification.
async fn three(app: &App) {
    for title in ["Themes", "Docs", "Search"] {
        agent(
            app,
            "POST",
            "/api/v1/items",
            json!({"repo": REPO, "title": title}),
        )
        .await;
    }
}

async fn list(app: &App, query: &str) -> Value {
    let (code, answer) = call(app, "GET", &format!("/api/v1/notifications{query}"), None).await;
    assert_eq!(code, StatusCode::OK, "{answer}");
    answer
}

fn keys(list: &Value) -> Vec<&str> {
    list["notifications"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["subject"]["item"]["key"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn the_list_pages_newest_first_with_its_total_and_counts() {
    let app = app();
    three(&app).await;
    let first = list(&app, "?limit=2").await;
    assert_eq!(keys(&first), ["PIN-3", "PIN-2"]);
    assert_eq!(first["total"], 3);
    assert_eq!(first["counts"], json!({"waiting": 0, "unread": 3}));
    let newest = common::db(&app)
        .events_after(0, 1000)
        .unwrap()
        .last()
        .unwrap()
        .id;
    assert_eq!(first["up_to"], newest);
    assert_eq!(keys(&list(&app, "?limit=2&offset=2").await), ["PIN-1"]);

    let row = &first["notifications"][0];
    assert_eq!(row["reason"], "item_added");
    assert_eq!(row["subject"]["item"]["title"], "Search");
    assert_eq!(row["subject"]["item"]["status"], "open");
    assert_eq!(row["repo"], REPO);
    assert_eq!(row["actor"]["kind"], "agent");
    assert_eq!(
        (&row["unread"], &row["read"], &row["done"]),
        (&json!(1), &json!(false), &json!(false))
    );

    let (_, counts) = call(&app, "GET", "/api/v1/notifications/counts", None).await;
    assert_eq!(counts, json!({"waiting": 0, "unread": 3}));
    let (code, _) = call(&app, "GET", "/api/v1/notifications?tab=later", None).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn the_person_marks_a_notification_read_unread_done_and_back() {
    let app = app();
    three(&app).await;
    let id = list(&app, "?limit=1").await["notifications"][0]["id"].clone();
    let path = |what: &str| format!("/api/v1/notifications/{id}/{what}");
    let mut live = app.state.events().subscribe();

    assert_eq!(
        call(&app, "PUT", &path("read"), None).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(keys(&list(&app, "?tab=unread").await), ["PIN-2", "PIN-1"]);
    // every window hears of it, with the notification and the counts
    let notice = live.try_recv().unwrap();
    assert_eq!(notice.kind, "notification_changed");
    assert_eq!(notice.notification.unwrap()["read"], true);
    assert_eq!(notice.counts.unwrap(), json!({"waiting": 0, "unread": 2}));

    call(&app, "DELETE", &path("read"), None).await;
    assert_eq!(list(&app, "?tab=unread").await["total"], 3);

    call(&app, "PUT", &path("done"), None).await;
    assert_eq!(keys(&list(&app, "").await), ["PIN-2", "PIN-1"]);
    call(&app, "DELETE", &path("done"), None).await;
    assert_eq!(keys(&list(&app, "").await), ["PIN-3", "PIN-2", "PIN-1"]);

    let (code, _) = call(&app, "PUT", "/api/v1/notifications/999/read", None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_the_person_changes_a_notification() {
    let app = app();
    three(&app).await;
    let id = list(&app, "?limit=1").await["notifications"][0]["id"].clone();
    for (method, path) in [
        ("PUT", format!("/api/v1/notifications/{id}/read")),
        ("DELETE", format!("/api/v1/notifications/{id}/read")),
        ("PUT", format!("/api/v1/notifications/{id}/done")),
        ("DELETE", format!("/api/v1/notifications/{id}/done")),
        ("POST", "/api/v1/notifications/read".to_string()),
    ] {
        let (code, _) =
            common::call_as(&app, method, &path, Some(json!({"up_to": 1000})), &[]).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
    assert_eq!(list(&app, "").await["counts"]["unread"], 3);
}

#[tokio::test]
async fn reading_all_leaves_an_event_after_the_list_unread() {
    let app = app();
    three(&app).await;
    let shown = list(&app, "?tab=unread").await;
    // an agent comments after the list was read
    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        json!({"body": "Started."}),
    )
    .await;
    let (code, answer) = call(
        &app,
        "POST",
        "/api/v1/notifications/read",
        Some(json!({"tab": "unread", "up_to": shown["up_to"]})),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{answer}");
    assert_eq!(answer["read"], 2);
    let unread = list(&app, "?tab=unread").await;
    assert_eq!(keys(&unread), ["PIN-1"]);
    assert_eq!(unread["notifications"][0]["unread"], 2);
}

#[tokio::test]
async fn a_waiting_notification_is_not_marked_done() {
    let app = app();
    let review = agent(
        &app,
        "POST",
        "/api/v1/reviews",
        json!({"kind": "question", "title": "Deploy?",
               "origin": {"repo": REPO, "ref": "main"},
               "requested_by": "claude-code", "session": "s-1",
               "payload": {"questions": [{"id": "q1", "question": "Deploy?", "answer": "yes-no"}]}}),
    )
    .await;
    let subject =
        pinrail_core::db::EventSubject::Review(review["id"].as_str().unwrap().to_string());
    let id = common::db(&app)
        .report_notification(&pinrail_core::db::Notify {
            subject: &subject,
            reason: "question_asked",
            waiting: true,
            event: 1,
            actor: Some("claude-code"),
            summary: None,
            at: chrono::Utc::now(),
        })
        .unwrap()
        .unwrap();
    let (code, _) = call(
        &app,
        "PUT",
        &format!("/api/v1/notifications/{id}/done"),
        None,
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT);
}
