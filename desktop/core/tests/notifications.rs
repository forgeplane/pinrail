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

/// An item, a session and a review, as the subjects of notifications.
async fn subjects(app: &App) -> [EventSubject; 3] {
    let item = agent(
        app,
        "POST",
        "/api/v1/items",
        json!({"repo": REPO, "title": "Work"}),
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
