//! The person's notifications as stored: one for each subject, its unread
//! count, read, unread and done, and its deletion with its subject.

use chrono::Utc;
use pinrail_core::db::{Counts, Db, EventSubject, Listed, Notify};
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

/// Reports event `event` on `subject`, by claude-code.
fn report(db: &Db, subject: &EventSubject, reason: &str, event: i64) -> Option<i64> {
    db.report_notification(&Notify {
        subject,
        reason,
        waiting: false,
        event,
        actor: Some("claude-code"),
        summary: Some("A line"),
        at: Utc::now(),
    })
    .unwrap()
}

/// An item, a session and a review, as the subjects of notifications. The
/// person adds the item, which reports nothing to them.
async fn subjects(app: &App) -> [EventSubject; 3] {
    let (_, item) = call(
        app,
        "POST",
        "/api/v1/items",
        Some(json!({"repo": REPO, "title": "Work"})),
    )
    .await;
    agent(
        app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        json!({"text": "Working"}),
    )
    .await;
    let review = agent(
        app,
        "POST",
        "/api/v1/reviews",
        json!({"kind": "question", "title": "Deploy?",
               "origin": {"repo": REPO, "ref": "main"},
               "requested_by": "claude-code", "session": "s-1",
               "payload": {"questions": [{"id": "q1", "question": "Deploy?", "answer": "yes-no"}]}}),
    )
    .await;
    [
        EventSubject::Item(item["id"].as_str().unwrap().to_string()),
        EventSubject::Session(common::sid(app, "claude-code", "s-1")),
        EventSubject::Review(review["id"].as_str().unwrap().to_string()),
    ]
}

#[tokio::test]
async fn one_notification_per_subject_counts_its_unread_events() {
    let app = app();
    let db = common::db(&app);
    let [item, session, _] = subjects(&app).await;

    let id = report(&db, &item, "item_added", 10).unwrap();
    assert_eq!(report(&db, &item, "item_commented", 11), Some(id));
    report(&db, &session, "agent_finished", 12).unwrap();

    let row = db.notification(id).unwrap().unwrap();
    assert_eq!(row.subject, item);
    assert_eq!(row.repo.as_deref(), Some(REPO));
    assert_eq!(row.reason, "item_commented");
    assert_eq!(row.last_event, 11);
    assert_eq!(row.last_actor.as_deref(), Some("claude-code"));
    assert_eq!(row.unread, 2);
    assert_eq!(
        db.notification_counts().unwrap(),
        Counts {
            waiting: 0,
            unread: 2
        }
    );
    // newest first
    let listed: Vec<i64> = db
        .notifications(Listed::Unread, 10)
        .unwrap()
        .iter()
        .map(|n| n.last_event)
        .collect();
    assert_eq!(listed, [12, 11]);
}

#[tokio::test]
async fn a_notification_is_read_unread_and_done_until_the_next_event() {
    let app = app();
    let db = common::db(&app);
    let [item, ..] = subjects(&app).await;
    let id = report(&db, &item, "item_added", 10).unwrap();

    assert!(db.mark_notification_read(id).unwrap());
    let row = db.notification(id).unwrap().unwrap();
    assert_eq!((row.unread, row.read_event), (0, 10));
    assert!(db.notifications(Listed::Unread, 10).unwrap().is_empty());
    assert_eq!(db.notifications(Listed::Open, 10).unwrap().len(), 1);

    assert!(db.mark_notification_unread(id).unwrap());
    assert_eq!(db.notification(id).unwrap().unwrap().unread, 1);

    assert!(db.mark_notification_done(id, Utc::now()).unwrap());
    assert!(db.notifications(Listed::Open, 10).unwrap().is_empty());
    assert_eq!(db.notification_counts().unwrap().unread, 0);

    // the next event brings it back, with that event alone unread
    assert_eq!(report(&db, &item, "item_commented", 11), Some(id));
    let row = db.notification(id).unwrap().unwrap();
    assert_eq!((row.unread, row.done_at), (1, None));
    assert_eq!(db.notifications(Listed::Open, 10).unwrap().len(), 1);

    assert!(!db.mark_notification_read(id + 100).unwrap());
}

#[tokio::test]
async fn a_waiting_notification_is_listed_and_counted_apart() {
    let app = app();
    let db = common::db(&app);
    let [_, _, review] = subjects(&app).await;
    let id = db
        .report_notification(&Notify {
            subject: &review,
            reason: "question_asked",
            waiting: true,
            event: 5,
            actor: Some("claude-code"),
            summary: None,
            at: Utc::now(),
        })
        .unwrap()
        .unwrap();
    db.mark_notification_read(id).unwrap();
    assert_eq!(db.notification_counts().unwrap().waiting, 1);
    assert_eq!(db.notifications(Listed::Waiting, 10).unwrap()[0].id, id);
}

#[tokio::test]
async fn a_notification_goes_with_its_item_session_or_review() {
    let app = app();
    let db = common::db(&app);
    let [item, session, review] = subjects(&app).await;
    let ids = [&item, &session, &review].map(|s| report(&db, s, "a_reason", 1).unwrap());

    let (code, _) = call(&app, "DELETE", "/api/v1/items/PIN-1", None).await;
    assert!(code.is_success(), "{code}");
    assert_eq!(db.notification(ids[0]).unwrap(), None);

    let EventSubject::Review(review) = review else {
        unreachable!()
    };
    let (code, _) = call(&app, "DELETE", &format!("/api/v1/reviews/{review}"), None).await;
    assert!(code.is_success(), "{code}");
    assert_eq!(db.notification(ids[2]).unwrap(), None);

    let EventSubject::Session(session) = session else {
        unreachable!()
    };
    let path = format!("/api/v1/sessions/claude-code/{session}");
    let (code, _) = call(&app, "DELETE", &path, None).await;
    assert!(code.is_success(), "{code}");
    assert_eq!(db.notification(ids[1]).unwrap(), None);
}

/// The open notifications, each as its item's key, reason and unread count.
fn listed(app: &App) -> Vec<(String, String, i64)> {
    let db = common::db(app);
    db.notifications(Listed::Open, 100)
        .unwrap()
        .into_iter()
        .map(|n| {
            let EventSubject::Item(id) = &n.subject else {
                panic!("{n:?}")
            };
            let key = db
                .item(&pinrail_core::items::ItemRef::Id(id.clone()), None)
                .unwrap()
                .unwrap()
                .key
                .to_string();
            (key, n.reason, n.unread)
        })
        .collect()
}

fn row(key: &str, reason: &str, unread: i64) -> (String, String, i64) {
    (key.to_string(), reason.to_string(), unread)
}

async fn add_item(app: &App, title: &str) {
    agent(
        app,
        "POST",
        "/api/v1/items",
        json!({"repo": REPO, "title": title}),
    )
    .await;
}

#[tokio::test]
async fn an_agents_change_to_an_item_is_reported_with_its_reason() {
    let app = app();
    add_item(&app, "Themes").await;
    assert_eq!(listed(&app), [row("PIN-1", "item_added", 1)]);

    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        json!({"body": "The loader is ready."}),
    )
    .await;
    assert_eq!(listed(&app), [row("PIN-1", "item_commented", 2)]);
    let db = common::db(&app);
    let notification = &db.notifications(Listed::Open, 1).unwrap()[0];
    assert_eq!(
        notification.summary.as_deref(),
        Some("The loader is ready.")
    );
    assert_eq!(notification.last_actor.as_deref(), Some("claude-code"));

    agent(
        &app,
        "PATCH",
        "/api/v1/items/PIN-1",
        json!({"title": "Load themes"}),
    )
    .await;
    assert_eq!(listed(&app), [row("PIN-1", "item_changed", 3)]);

    agent(&app, "POST", "/api/v1/items/PIN-1/done", json!({})).await;
    assert_eq!(listed(&app), [row("PIN-1", "item_closed", 4)]);
}

#[tokio::test]
async fn a_breakdown_under_an_item_is_one_notification_on_it() {
    let app = app();
    add_item(&app, "Themes").await;
    agent(
        &app,
        "POST",
        "/api/v1/items",
        json!({"repo": REPO, "parent": "PIN-1", "items": [
            {"ref": "a", "title": "Read"},
            {"ref": "b", "title": "Parse"},
            {"ref": "c", "title": "Apply"},
        ]}),
    )
    .await;
    assert_eq!(listed(&app), [row("PIN-1", "item_added", 4)]);
    let db = common::db(&app);
    let notification = &db.notifications(Listed::Open, 1).unwrap()[0];
    assert_eq!(notification.summary.as_deref(), Some("PIN-4 Apply"));
}

#[tokio::test]
async fn the_persons_own_change_marks_the_notification_read() {
    let app = app();
    add_item(&app, "Themes").await;
    let (code, _) = call(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        Some(json!({"body": "Use the new format."})),
    )
    .await;
    assert!(code.is_success());
    assert_eq!(listed(&app), [row("PIN-1", "item_added", 0)]);

    // an item the person adds is not reported to them
    call(
        &app,
        "POST",
        "/api/v1/items",
        Some(json!({"repo": REPO, "title": "Docs"})),
    )
    .await;
    assert_eq!(listed(&app).len(), 1);
}

#[tokio::test]
async fn a_done_notification_comes_back_with_the_next_change() {
    let app = app();
    add_item(&app, "Themes").await;
    let db = common::db(&app);
    let id = db.notifications(Listed::Open, 1).unwrap()[0].id;
    db.mark_notification_done(id, Utc::now()).unwrap();
    assert!(listed(&app).is_empty());

    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        json!({"body": "Started."}),
    )
    .await;
    assert_eq!(listed(&app), [row("PIN-1", "item_commented", 1)]);
}

#[tokio::test]
async fn a_change_whose_notification_fails_is_not_made() {
    let app = app();
    add_item(&app, "Themes").await;
    let events = common::db(&app).events_after(0, 1000).unwrap().len();
    rusqlite::Connection::open(app.state.config().db_path())
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER refuse_notifications BEFORE UPDATE ON notifications
               BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .unwrap();
    let (code, _) = common::call_as(
        &app,
        "PATCH",
        "/api/v1/items/PIN-1",
        Some(json!({"title": "Load themes", "by": "claude-code", "session": "s-1"})),
        &[],
    )
    .await;
    assert!(code.is_server_error(), "{code}");
    let (_, item) = call(&app, "GET", "/api/v1/items/PIN-1", None).await;
    assert_eq!(item["title"], "Themes");
    assert_eq!(
        common::db(&app).events_after(0, 1000).unwrap().len(),
        events
    );
}

#[tokio::test]
async fn a_live_notice_carries_the_notification_and_the_counts() {
    let app = app();
    let mut live = app.state.events().subscribe();
    add_item(&app, "Themes").await;
    let notice = std::iter::from_fn(|| live.try_recv().ok())
        .find(|n| n.kind == "item_changed")
        .unwrap();
    let notification = notice.notification.unwrap();
    assert_eq!(notification["reason"], "item_added");
    assert_eq!(notification["unread"], 1);
    assert_eq!(notice.counts.unwrap(), json!({"waiting": 0, "unread": 1}));
}
