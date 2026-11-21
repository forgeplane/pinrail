//! A notice replayed to a client that reconnects is the one sent live.

use serde_json::{Value, json};

mod common;
use common::{App, app, call};

const REPO: &str = "forgeplane/pinrail";

/// A notice as a client receives it, without its review and its subject's
/// key: a replay gives them as they are now, which may have changed since.
fn shape(notice: &pinrail_core::events::Notice) -> Value {
    let mut value = serde_json::to_value(notice).unwrap();
    value.as_object_mut().unwrap().remove("review");
    if let Some(subject) = value["subject"].as_object_mut() {
        subject.remove("key");
    }
    value
}

async fn agent(app: &App, method: &str, path: &str, body: Value) -> Value {
    let mut body = body;
    body["by"] = json!("claude-code");
    body["session"] = json!("s-1");
    let (code, answer) = common::call_as(app, method, path, Some(body), &[]).await;
    assert!(code.is_success(), "{method} {path}: {code} {answer}");
    answer
}

async fn person(app: &App, method: &str, path: &str, body: Option<Value>) -> Value {
    let (code, answer) = call(app, method, path, body).await;
    assert!(code.is_success(), "{method} {path}: {code} {answer}");
    answer
}

#[tokio::test]
async fn every_replayed_notice_matches_the_live_one() {
    let app = app();
    let mut live = app.state.events().subscribe();
    let status = "/api/v1/sessions/claude-code/s-1/status";

    agent(&app, "POST", status, json!({"text": "Working"})).await;
    let sent = person(
        &app,
        "POST",
        &format!(
            "/api/v1/sessions/claude-code/{}/messages",
            common::sid(&app, "claude-code", "s-1")
        ),
        Some(json!({"text": "Use the new endpoint."})),
    )
    .await;
    let deleted = format!(
        "/api/v1/sessions/claude-code/{}/messages/{}",
        common::sid(&app, "claude-code", "s-1"),
        sent["id"]
    );
    person(&app, "DELETE", &deleted, None).await;
    person(
        &app,
        "POST",
        &format!(
            "/api/v1/sessions/claude-code/{}/messages",
            common::sid(&app, "claude-code", "s-1")
        ),
        Some(json!({"text": "Run the tests too."})),
    )
    .await;
    // hands the message over, and the CLI acknowledges it
    let handed = agent(&app, "POST", status, json!({"text": "Still working"})).await;
    agent(
        &app,
        "POST",
        "/api/v1/sessions/claude-code/s-1/messages/ack",
        json!({"ids": [handed["messages"][0]["id"]]}),
    )
    .await;
    let archive = &format!(
        "/api/v1/sessions/claude-code/{}/archive",
        common::sid(&app, "claude-code", "s-1")
    );
    person(&app, "PUT", archive, None).await;
    person(&app, "DELETE", archive, None).await;
    agent(
        &app,
        "POST",
        "/api/v1/items",
        json!({"repo": REPO, "title": "Work"}),
    )
    .await;
    agent(
        &app,
        "PATCH",
        "/api/v1/items/PIN-1",
        json!({"claim": "claim"}),
    )
    .await;
    let gone = agent(
        &app,
        "POST",
        "/api/v1/items",
        json!({"repo": REPO, "title": "Gone"}),
    )
    .await;
    person(
        &app,
        "PATCH",
        "/api/v1/repos/forgeplane%2Fpinrail",
        Some(json!({"prefix": "PR"})),
    )
    .await;
    person(&app, "DELETE", "/api/v1/items/PR-2", None).await;
    person(
        &app,
        "PATCH",
        "/api/v1/settings",
        Some(json!({"appearance": {"theme": "dark"}})),
    )
    .await;

    let mut sent_live = Vec::new();
    while let Ok(notice) = live.try_recv() {
        sent_live.push(notice);
    }
    let replayed = app.state.events().after(0, 1000).unwrap();
    let kinds: Vec<&str> = sent_live.iter().map(|n| n.kind.as_str()).collect();
    for kind in [
        "session_status",
        "session_message_sent",
        "session_message_deleted",
        "session_messages_delivered",
        "session_archived",
        "session_unarchived",
        "item_changed",
        "item_deleted",
        "prefix_changed",
        "settings_changed",
    ] {
        assert!(kinds.contains(&kind), "no {kind} notice in {kinds:?}");
    }
    for notice in &sent_live {
        // the events about an item are deleted with it
        if notice.subject["id"] == gone["id"] {
            assert!(replayed.iter().all(|r| r.event_id != notice.event_id));
            continue;
        }
        let again = replayed
            .iter()
            .find(|r| r.event_id == notice.event_id)
            .unwrap_or_else(|| panic!("{} {} was not replayed", notice.kind, notice.event_id));
        assert_eq!(shape(again), shape(notice), "the replay of {}", notice.kind);
    }
}

#[tokio::test]
async fn an_event_about_an_item_keeps_its_subject_and_actor_and_goes_with_the_item() {
    use pinrail_core::db::{EventActor, EventSubject, NewEvent};
    let app = app();
    let item = agent(
        &app,
        "POST",
        "/api/v1/items",
        json!({"repo": REPO, "title": "Work"}),
    )
    .await;
    let id = item["id"].as_str().unwrap().to_string();
    let session = common::sid(&app, "claude-code", "s-1");
    let db = common::db(&app);
    let event = db
        .append_event_as(&NewEvent {
            kind: "item_changed",
            subject: Some(EventSubject::Item(id.clone())),
            actor: EventActor::Agent {
                name: "claude-code".to_string(),
                session: Some(session.clone()),
            },
            attrs: &json!({"key": "PIN-1"}),
        })
        .unwrap();
    let stored = db
        .events_after(event - 1, 1)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(stored.id, event);
    assert_eq!(
        stored.subject_json(),
        json!({"kind": "item", "id": id, "key": "PIN-1"})
    );
    assert_eq!(
        stored.actor_json(),
        json!({"kind": "agent", "name": "claude-code", "session": session})
    );
    // an event written before the columns were there has neither
    let old = db
        .events_after(0, 1000)
        .unwrap()
        .into_iter()
        .find(|e| e.id != event)
        .unwrap();
    assert_eq!(old.subject_json(), Value::Null);
    assert_eq!(old.actor_json(), Value::Null);

    person(&app, "DELETE", "/api/v1/items/PIN-1", None).await;
    assert!(
        db.events_after(0, 1000)
            .unwrap()
            .iter()
            .all(|e| e.id != event),
        "the item's event outlived it"
    );
    // the deletion's own event comes after it, and does not take its id,
    // which a client may have seen already
    let deleted = db
        .events_after(0, 1000)
        .unwrap()
        .into_iter()
        .find(|e| e.kind == "item_deleted")
        .unwrap();
    assert!(deleted.id > event, "{} reused {event}", deleted.id);
}

/// The `item_changed` events stored after `after`, each as its item's key
/// and what changed.
fn item_changes(app: &App, after: i64) -> Vec<(String, String)> {
    common::db(app)
        .events_after(after, 1000)
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == "item_changed")
        .map(|e| {
            let text = |name: &str| e.attrs[name].as_str().unwrap_or_default().to_string();
            (text("key"), text("change"))
        })
        .collect()
}

fn last_event(app: &App) -> i64 {
    common::db(app)
        .events_after(0, 100_000)
        .unwrap()
        .last()
        .map_or(0, |e| e.id)
}

async fn upload(app: &App, bytes: &[u8]) -> String {
    use axum::body::Body;
    use axum::http::Request;
    use sha2::Digest;
    use tower::ServiceExt;
    let hash = format!("{:x}", sha2::Sha256::digest(bytes));
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/attachments/{hash}"))
        .header("host", "127.0.0.1:4747")
        .header("content-type", "application/octet-stream")
        .header("content-length", bytes.len().to_string())
        .body(Body::from(bytes.to_vec()))
        .unwrap();
    let answer = app.router.clone().oneshot(request).await.unwrap();
    assert!(answer.status().is_success(), "{}", answer.status());
    hash
}

#[tokio::test]
async fn each_change_of_an_item_is_an_event_that_says_what_changed() {
    let app = app();
    let mut live = app.state.events().subscribe();
    let items = "/api/v1/items";
    agent(
        &app,
        "POST",
        items,
        json!({"repo": REPO, "title": "Themes"}),
    )
    .await;
    agent(
        &app,
        "POST",
        items,
        json!({"repo": REPO, "title": "Load themes", "parent": "PIN-1"}),
    )
    .await;
    agent(&app, "POST", items, json!({"repo": REPO, "title": "Docs"})).await;
    let comment = agent(
        &app,
        "POST",
        "/api/v1/items/PIN-1/comments",
        json!({"body": "word ".repeat(60)}),
    )
    .await;
    let pin1 = "/api/v1/items/PIN-1";
    agent(&app, "PATCH", pin1, json!({"title": "Load the themes"})).await;
    agent(&app, "PATCH", pin1, json!({"claim": "claim"})).await;
    agent(&app, "PATCH", pin1, json!({"claim": "release"})).await;
    person(&app, "PATCH", pin1, Some(json!({"status": "on_hold"}))).await;
    person(&app, "PATCH", pin1, Some(json!({"status": "open"}))).await;
    person(&app, "PUT", "/api/v1/items/PIN-1/queue", Some(json!({}))).await;
    person(&app, "DELETE", "/api/v1/items/PIN-1/queue", None).await;
    agent(&app, "PATCH", pin1, json!({"depends_on": ["PIN-3"]})).await;
    let hash = upload(&app, b"error: the watcher stopped").await;
    agent(
        &app,
        "PATCH",
        "/api/v1/items/PIN-3",
        json!({"body": "The log: [crash](attachment:crash.log)",
               "attachments": {"crash.log": {"sha256": hash, "size": 26}}}),
    )
    .await;
    agent(
        &app,
        "PATCH",
        "/api/v1/items/PIN-3",
        json!({"repo": "forgeplane/docs"}),
    )
    .await;
    agent(&app, "POST", "/api/v1/items/PIN-2/done", json!({})).await;

    let changes = item_changes(&app, 0);
    let expected = [
        ("PIN-1", "created"),
        ("PIN-2", "created"),
        ("PIN-3", "created"),
        ("PIN-1", "commented"),
        ("PIN-1", "edited"),
        ("PIN-1", "claimed"),
        ("PIN-1", "released"),
        ("PIN-1", "held"),
        ("PIN-1", "status"),
        ("PIN-1", "queued"),
        ("PIN-1", "unqueued"),
        ("PIN-1", "depends"),
        ("PIN-3", "edited"),
        ("PIN-3", "files"),
        ("DOC-1", "moved"),
        ("PIN-2", "status"),
        ("PIN-1", "status"),
    ];
    let expected: Vec<(String, String)> = expected
        .iter()
        .map(|(k, c)| (k.to_string(), c.to_string()))
        .collect();
    assert_eq!(changes, expected);

    let events: Vec<_> = common::db(&app)
        .events_after(0, 1000)
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == "item_changed")
        .collect();
    let session = common::sid(&app, "claude-code", "s-1");
    // a new sub-item names its parent
    assert_eq!(events[1].attrs["parent"], "PIN-1");
    assert!(events[0].attrs.get("parent").is_none());
    // a comment is named, with the start of its text
    assert_eq!(events[3].attrs["comment"], comment["id"]);
    let excerpt = events[3].attrs["excerpt"].as_str().unwrap();
    assert!(excerpt.chars().count() <= 201, "{excerpt}");
    assert!(excerpt.ends_with('…'));
    // a status says what it was and what it is
    assert_eq!(
        (&events[8].attrs["from"], &events[8].attrs["to"]),
        (&json!("on_hold"), &json!("open"))
    );
    // the agent changed what it changed, and Pinrail closed the parent
    assert_eq!(
        events[3].actor_json(),
        json!({"kind": "agent", "name": "claude-code", "session": session})
    );
    assert_eq!(events[3].attrs["agent"], "claude-code");
    assert_eq!(events[3].attrs["session"], json!(session));
    assert_eq!(events[7].actor_json()["kind"], "person");
    assert_eq!(events[16].actor_json(), json!({"kind": "system"}));
    assert_eq!(events[16].attrs["to"], "done");
    // each is about its item
    let pin1 = events[0].item_id.clone().unwrap();
    assert_eq!(
        events[4].subject_json(),
        json!({"kind": "item", "id": pin1, "key": "PIN-1"})
    );

    // the notices sent live are the events stored, in order
    let mut sent = Vec::new();
    while let Ok(notice) = live.try_recv() {
        if notice.kind == "item_changed" {
            sent.push(notice.event_id);
        }
    }
    assert_eq!(sent, events.iter().map(|e| e.id).collect::<Vec<_>>());
}

#[tokio::test]
async fn a_breakdown_stores_and_sends_an_event_for_each_item() {
    let app = app();
    let mut live = app.state.events().subscribe();
    agent(
        &app,
        "POST",
        "/api/v1/items",
        json!({"repo": REPO, "items": [
            {"ref": "a", "title": "A"},
            {"ref": "b", "title": "B", "parent": "a"},
            {"ref": "c", "title": "C", "depends_on": ["b"]},
        ]}),
    )
    .await;
    let created = [
        ("PIN-1", "created"),
        ("PIN-2", "created"),
        ("PIN-3", "created"),
    ]
    .map(|(k, c)| (k.to_string(), c.to_string()));
    assert_eq!(item_changes(&app, 0), created);
    let mut sent = 0;
    while let Ok(notice) = live.try_recv() {
        sent += usize::from(notice.kind == "item_changed");
    }
    assert_eq!(sent, 3);
}

#[tokio::test]
async fn an_item_change_whose_event_fails_is_not_made() {
    let app = app();
    agent(
        &app,
        "POST",
        "/api/v1/items",
        json!({"repo": REPO, "title": "Themes"}),
    )
    .await;
    let before = last_event(&app);
    let timeline = |app: &App| -> i64 {
        let conn = rusqlite::Connection::open(app.state.config().db_path()).unwrap();
        conn.query_row("SELECT count(*) FROM work_item_events", [], |r| r.get(0))
            .unwrap()
    };
    let entries = timeline(&app);
    rusqlite::Connection::open(app.state.config().db_path())
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER refuse_item_events BEFORE INSERT ON events
               WHEN NEW.kind = 'item_changed'
               BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .unwrap();

    let (code, _) = call(
        &app,
        "PATCH",
        "/api/v1/items/PIN-1",
        Some(json!({"title": "Load the themes"})),
    )
    .await;
    assert!(code.is_server_error(), "{code}");
    let (_, item) = call(&app, "GET", "/api/v1/items/PIN-1", None).await;
    assert_eq!(item["title"], "Themes");
    assert_eq!(timeline(&app), entries);
    assert_eq!(last_event(&app), before);
}
