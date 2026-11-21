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
    // the question's own notification is left out: each test reports
    // what it checks
    rusqlite::Connection::open(app.state.config().db_path())
        .unwrap()
        .execute("DELETE FROM notifications", [])
        .unwrap();
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
    // any other change is off until the person turns it on
    settings(&app, json!({"inbox": {"show": {"item_changed": true}}})).await;
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
        "POST",
        "/api/v1/items/PIN-1/comments",
        Some(json!({"body": "Started.", "by": "claude-code", "session": "s-1"})),
        &[],
    )
    .await;
    assert!(code.is_server_error(), "{code}");
    let (_, item) = call(&app, "GET", "/api/v1/items/PIN-1", None).await;
    let comments = item["timeline"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "comment")
        .count();
    assert_eq!(comments, 0, "{item}");
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

async fn settings(app: &App, change: Value) {
    let (code, answer) = call(app, "PATCH", "/api/v1/settings", Some(change)).await;
    assert!(code.is_success(), "{code} {answer}");
}

#[tokio::test]
async fn a_kind_the_person_turned_off_adds_nothing_and_its_event_is_kept() {
    let app = app();
    settings(&app, json!({"inbox": {"show": {"item_commented": false}}})).await;
    add_item(&app, "Themes").await;
    agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        json!({"body": "The loader is ready."}),
    )
    .await;
    assert_eq!(listed(&app), [row("PIN-1", "item_added", 1)]);
    let commented = common::db(&app)
        .events_after(0, 1000)
        .unwrap()
        .into_iter()
        .filter(|e| e.attrs["change"] == "commented")
        .count();
    assert_eq!(commented, 1);

    // any other change is off unless the person turns it on
    agent(
        &app,
        "PATCH",
        "/api/v1/items/PIN-1",
        json!({"title": "Load themes"}),
    )
    .await;
    assert_eq!(listed(&app), [row("PIN-1", "item_added", 1)]);
}

/// A review of the list plugin an agent submits, revising `revises` when
/// given, and its id.
async fn submit_review(app: &App, revises: Option<&str>) -> String {
    let mut body = json!({
        "plugin": "list", "title": "Rate limiter",
        "origin": {"repo": REPO, "ref": "main"},
        "requested_by": "claude-code",
        "payload": {"groups": [{"title": "a", "items": [{"id": 1, "title": "one"}]}]}
    });
    if let Some(revises) = revises {
        body["revises"] = json!(revises);
    }
    let review = agent(app, "POST", "/api/v1/reviews", body).await;
    review["id"].as_str().unwrap().to_string()
}

/// The review's notification, as stored.
fn of_review(app: &App, review: &str) -> Option<pinrail_core::db::Notification> {
    common::db(app)
        .notification_of(0, Some(&EventSubject::Review(review.to_string())))
        .unwrap()
}

#[tokio::test]
async fn a_review_waits_until_the_person_decides_it() {
    let app = app();
    let id = submit_review(&app, None).await;
    let n = of_review(&app, &id).unwrap();
    assert_eq!(
        (n.reason.as_str(), n.waiting, n.unread),
        ("review_requested", true, 1)
    );
    assert_eq!(common::db(&app).notification_counts().unwrap().waiting, 1);

    call(&app, "POST", &format!("/api/v1/reviews/{id}/viewed"), None).await;
    let n = of_review(&app, &id).unwrap();
    assert_eq!((n.waiting, n.unread), (true, 0));

    let (code, answer) = call(
        &app,
        "POST",
        &format!("/api/v1/reviews/{id}/decision"),
        Some(json!({"data": {"decisions": [{"id": 1, "action": "accept"}], "undecided": []}})),
    )
    .await;
    assert!(code.is_success(), "{answer}");
    let n = of_review(&app, &id).unwrap();
    assert!(!n.waiting && n.done_at.is_some());
    assert_eq!(common::db(&app).notification_counts().unwrap().waiting, 0);
}

#[tokio::test]
async fn a_new_round_takes_over_the_notification_of_the_one_before() {
    let app = app();
    let first = submit_review(&app, None).await;
    let before = of_review(&app, &first).unwrap();
    let second = submit_review(&app, Some(&first)).await;
    assert!(of_review(&app, &first).is_none());
    let n = of_review(&app, &second).unwrap();
    assert_eq!(n.id, before.id);
    assert_eq!((n.reason.as_str(), n.waiting), ("new_round", true));
}

#[tokio::test]
async fn a_question_waits_and_a_discarded_one_is_done() {
    let app = app();
    let question = agent(
        &app,
        "POST",
        "/api/v1/reviews",
        json!({"kind": "question", "title": "Deploy?",
               "origin": {"repo": REPO, "ref": "main"},
               "requested_by": "claude-code", "session": "s-1",
               "payload": {"questions": [{"id": "q1", "question": "Deploy?", "answer": "yes-no"}]}}),
    )
    .await;
    let id = question["id"].as_str().unwrap().to_string();
    let n = of_review(&app, &id).unwrap();
    assert_eq!((n.reason.as_str(), n.waiting), ("question_asked", true));
    let (code, _) = call(&app, "POST", &format!("/api/v1/reviews/{id}/discard"), None).await;
    assert!(code.is_success());
    let n = of_review(&app, &id).unwrap();
    assert!(!n.waiting && n.done_at.is_some());
}

#[tokio::test]
async fn a_withdrawn_or_expired_review_stops_waiting_and_says_why() {
    let app = app();
    let withdrawn = submit_review(&app, None).await;
    let (code, answer) = common::call_as(
        &app,
        "POST",
        &format!("/api/v1/reviews/{withdrawn}/withdraw"),
        Some(json!({"reason": "superseded", "by": "claude-code"})),
        &[],
    )
    .await;
    assert!(code.is_success(), "{answer}");
    let n = of_review(&app, &withdrawn).unwrap();
    assert_eq!(
        (n.reason.as_str(), n.waiting, n.unread),
        ("review_withdrawn", false, 2)
    );
    assert_eq!(n.done_at, None);

    let mut body = json!({
        "plugin": "list", "title": "Late", "origin": {"repo": REPO},
        "requested_by": "claude-code", "expires_at": "2020-01-01T00:00:00Z",
        "payload": {"groups": []}
    });
    body["by"] = json!("claude-code");
    let (_, late) = common::call_as(&app, "POST", "/api/v1/reviews", Some(body), &[]).await;
    let late = late["id"].as_str().unwrap().to_string();
    app.state.reviews().sweep_expired().unwrap();
    let n = of_review(&app, &late).unwrap();
    assert_eq!((n.reason.as_str(), n.waiting), ("review_expired", false));
    assert_eq!(common::db(&app).notification_counts().unwrap().waiting, 0);
}

/// A report of claude-code's hooks in session s-1, `s` seconds from now.
async fn hook(app: &App, event: &str, s: i64, extra: Value) {
    let mut body = json!({
        "event": event,
        "at": (chrono::Utc::now() + chrono::Duration::milliseconds(s)).to_rfc3339(),
    });
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let (code, answer) = call(
        app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/reports",
        Some(body),
    )
    .await;
    assert!(code.is_success(), "{event}: {code} {answer}");
}

/// The notification of claude-code's session s-1.
fn of_session(app: &App) -> Option<pinrail_core::db::Notification> {
    let session = common::sid(app, "claude-code", "s-1");
    common::db(app)
        .notification_of(0, Some(&EventSubject::Session(session)))
        .unwrap()
}

#[tokio::test]
async fn an_agent_that_asks_for_a_permission_waits_until_it_moves_on() {
    let app = app();
    hook(&app, "prompt", 0, json!({})).await;
    hook(
        &app,
        "needs_input",
        10,
        json!({"wait": "permission", "detail": {"tool": "Bash", "summary": "cargo test"}}),
    )
    .await;
    let n = of_session(&app).unwrap();
    assert_eq!(
        (n.reason.as_str(), n.waiting),
        ("permission_requested", true)
    );
    assert_eq!(n.summary.as_deref(), Some("cargo test"));

    // answered in the terminal: the agent works again
    hook(&app, "working", 20, json!({})).await;
    let n = of_session(&app).unwrap();
    assert!(!n.waiting);
    assert_eq!(common::db(&app).notification_counts().unwrap().waiting, 0);
}

#[tokio::test]
async fn a_working_agents_reports_add_no_notification() {
    let app = app();
    hook(&app, "prompt", 0, json!({"detail": {"prompt": "Go"}})).await;
    for s in 1..5 {
        hook(&app, "working", s * 10, json!({})).await;
    }
    assert!(of_session(&app).is_none());
}

#[tokio::test]
async fn a_finished_turn_is_reported_until_the_person_sees_the_session() {
    let app = app();
    hook(&app, "prompt", 0, json!({})).await;
    hook(&app, "turn_ended", 10, json!({"end": "done"})).await;
    let n = of_session(&app).unwrap();
    assert_eq!((n.reason.as_str(), n.unread), ("agent_finished", 1));

    let session = common::sid(&app, "claude-code", "s-1");
    call(
        &app,
        "PUT",
        &format!("/api/v1/sessions/claude-code/{session}/seen"),
        None,
    )
    .await;
    assert_eq!(of_session(&app).unwrap().unread, 0);
    call(
        &app,
        "DELETE",
        &format!("/api/v1/sessions/claude-code/{session}/seen"),
        None,
    )
    .await;
    assert_eq!(of_session(&app).unwrap().unread, 1);
}

#[tokio::test]
async fn marking_a_sessions_notification_read_sees_the_session() {
    let app = app();
    hook(&app, "prompt", 0, json!({})).await;
    hook(&app, "turn_ended", 10, json!({"end": "done"})).await;
    let n = of_session(&app).unwrap();
    let (code, _) = call(
        &app,
        "PUT",
        &format!("/api/v1/notifications/{}/read", n.id),
        None,
    )
    .await;
    assert!(code.is_success());
    let session = common::sid(&app, "claude-code", "s-1");
    let presence = common::db(&app)
        .presence("claude-code", &session)
        .unwrap()
        .unwrap();
    assert!(presence.seen_at.is_some());
    assert_eq!(of_session(&app).unwrap().unread, 0);
}

#[tokio::test]
async fn an_agents_note_is_reported_with_its_first_line() {
    let app = app();
    agent(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/status",
        json!({"text": "# Summary of the first stage\n\nTwo screenshots.", "kind": "note"}),
    )
    .await;
    let n = of_session(&app).unwrap();
    assert_eq!(n.reason, "agent_note");
    assert_eq!(n.summary.as_deref(), Some("Summary of the first stage"));
}

/// Runs SQL on the app's database, as a crash or a bug might leave it.
fn corrupt(app: &App, sql: &str) {
    rusqlite::Connection::open(app.state.config().db_path())
        .unwrap()
        .execute_batch(sql)
        .unwrap();
}

#[tokio::test]
async fn a_pending_review_whose_notification_was_lost_waits_again() {
    let app = app();
    let id = submit_review(&app, None).await;
    corrupt(&app, "DELETE FROM notifications");
    assert!(app.state.notifications().reconcile().unwrap() > 0);
    let n = of_review(&app, &id).unwrap();
    assert_eq!((n.reason.as_str(), n.waiting), ("review_requested", true));

    corrupt(
        &app,
        "UPDATE notifications SET waiting = 0, done_at = '2026-01-01T00:00:00Z'",
    );
    app.state.notifications().reconcile().unwrap();
    let n = of_review(&app, &id).unwrap();
    assert!(n.waiting && n.done_at.is_none());
    // nothing left to repair
    assert_eq!(app.state.notifications().reconcile().unwrap(), 0);
}

#[tokio::test]
async fn an_expired_review_stops_waiting_without_the_expiry_sweep() {
    let app = app();
    let mut body = json!({
        "plugin": "list", "title": "Late", "origin": {"repo": REPO},
        "requested_by": "claude-code", "expires_at": "2020-01-01T00:00:00Z",
        "payload": {"groups": []}
    });
    body["by"] = json!("claude-code");
    let (_, late) = common::call_as(&app, "POST", "/api/v1/reviews", Some(body), &[]).await;
    let late = late["id"].as_str().unwrap().to_string();
    corrupt(&app, "UPDATE notifications SET waiting = 1");
    app.state.notifications().reconcile().unwrap();
    assert!(!of_review(&app, &late).unwrap().waiting);
}

#[tokio::test]
async fn a_waiting_agent_whose_notification_was_lost_waits_again() {
    let app = app();
    hook(&app, "prompt", 0, json!({})).await;
    hook(&app, "needs_input", 10, json!({"wait": "question"})).await;
    corrupt(&app, "DELETE FROM notifications");
    app.state.notifications().reconcile().unwrap();
    let n = of_session(&app).unwrap();
    assert_eq!((n.reason.as_str(), n.waiting), ("input_requested", true));

    // the agent moved on, and the notice of it was lost
    hook(&app, "working", 20, json!({})).await;
    corrupt(&app, "UPDATE notifications SET waiting = 1");
    app.state.notifications().reconcile().unwrap();
    assert!(!of_session(&app).unwrap().waiting);
}

#[tokio::test]
async fn a_start_on_older_data_shows_what_waits_and_what_finished_unseen() {
    let app = app();
    let review = submit_review(&app, None).await;
    hook(&app, "prompt", 0, json!({})).await;
    hook(&app, "needs_input", 10, json!({"wait": "permission"})).await;
    // a second session, finished and never seen
    let (code, _) = call(
        &app,
        "POST",
        "/api/v1/sessions/codex/s-2/reports",
        Some(json!({"event": "prompt", "at": chrono::Utc::now().to_rfc3339()})),
    )
    .await;
    assert!(code.is_success());
    call(
        &app,
        "POST",
        "/api/v1/sessions/codex/s-2/reports",
        Some(json!({"event": "turn_ended", "end": "done",
                    "at": (chrono::Utc::now() + chrono::Duration::milliseconds(10)).to_rfc3339()})),
    )
    .await;
    // as the data was before the notifications existed
    corrupt(&app, "DELETE FROM notifications");

    let app = common::open_copy(&app);
    let db = common::db(&app);
    assert_eq!(db.notification_counts().unwrap().waiting, 2);
    assert!(of_review(&app, &review).unwrap().waiting);
    assert_eq!(of_session(&app).unwrap().reason, "permission_requested");
    let finished = db
        .notification_of(
            0,
            Some(&EventSubject::Session(common::sid(&app, "codex", "s-2"))),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        (finished.reason.as_str(), finished.waiting, finished.unread),
        ("agent_finished", false, 1)
    );
}

#[tokio::test]
async fn the_sweep_marks_old_read_notifications_done_and_deletes_old_done_ones() {
    let app = app();
    // a review that waits, and work items in each state and age
    let review = submit_review(&app, None).await;
    for title in [
        "Fresh read",
        "Old read",
        "Old unread",
        "Done long ago",
        "Done lately",
    ] {
        add_item(&app, title).await;
    }
    let now = Utc::now();
    let days = |d: i64| pinrail_core::reviews::iso(now - chrono::Duration::days(d));
    let title_is =
        |title: &str| format!("item_id = (SELECT id FROM work_items WHERE title = '{title}')");
    corrupt(
        &app,
        &format!(
            "UPDATE notifications SET unread = 0, updated_at = '{}' WHERE {};
             UPDATE notifications SET unread = 0, updated_at = '{}' WHERE {};
             UPDATE notifications SET unread = 1, updated_at = '{}' WHERE {};
             UPDATE notifications SET unread = 0, done_at = '{}', updated_at = '{}' WHERE {};
             UPDATE notifications SET unread = 0, done_at = '{}', updated_at = '{}' WHERE {};
             UPDATE notifications SET unread = 0, updated_at = '{}' WHERE review_id IS NOT NULL;",
            days(2),
            title_is("Fresh read"),
            days(8),
            title_is("Old read"),
            days(8),
            title_is("Old unread"),
            days(31),
            days(40),
            title_is("Done long ago"),
            days(20),
            days(25),
            title_is("Done lately"),
            days(60),
        ),
    );

    app.state.notifications().sweep(now).unwrap();
    let state = |title: &str| {
        let db = common::db(&app);
        let id: Option<String> = rusqlite::Connection::open(app.state.config().db_path())
            .unwrap()
            .query_row("SELECT id FROM work_items WHERE title = ?1", [title], |r| {
                r.get(0)
            })
            .unwrap();
        db.notification_of(0, Some(&EventSubject::Item(id.unwrap())))
            .unwrap()
            .map(|n| if n.done_at.is_some() { "done" } else { "open" })
    };
    assert_eq!(state("Fresh read"), Some("open"));
    assert_eq!(state("Old read"), Some("done"));
    assert_eq!(state("Old unread"), Some("open"));
    assert_eq!(state("Done long ago"), None);
    assert_eq!(state("Done lately"), Some("done"));
    // what waits is never touched, however old
    let n = of_review(&app, &review).unwrap();
    assert!(n.waiting && n.done_at.is_none());
}
