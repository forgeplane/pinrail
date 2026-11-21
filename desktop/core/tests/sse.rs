//! Catch-up and live notices through the SSE transport.

use std::sync::Arc;

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use pinrail_core::{Config, Pinrail, api};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn next_event(body: &mut Body, buffered: &mut String) -> (i64, Value) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(end) = buffered.find("\n\n") {
                let event: String = buffered.drain(..end + 2).collect();
                let id = event
                    .lines()
                    .find_map(|line| line.strip_prefix("id: "))
                    .unwrap();
                let data = event
                    .lines()
                    .find_map(|line| line.strip_prefix("data: "))
                    .unwrap();
                return (id.parse().unwrap(), serde_json::from_str(data).unwrap());
            }
            let frame = body.frame().await.unwrap().unwrap();
            let bytes = frame.into_data().unwrap();
            buffered.push_str(std::str::from_utf8(&bytes).unwrap());
        }
    })
    .await
    .expect("the next SSE event did not arrive")
}

#[tokio::test]
async fn catch_up_precedes_live_events_and_suppresses_replayed_ids() {
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(Pinrail::open(Config::new(dir.path(), 0)).unwrap());
    app.settings().change(&json!({"autostart": true})).unwrap();
    let cursor = app.events().after(0, 1).unwrap()[0].event_id;
    app.settings()
        .change(&json!({"appearance": {"theme": "dark"}}))
        .unwrap();
    let replayed = app.events().after(cursor, 1).unwrap().remove(0);

    let response = api::router(app.clone())
        .oneshot(
            Request::get(format!("/api/v1/events?after={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");

    // Simulate a notice present in both the backlog and the subscribed channel.
    app.events().publish(replayed.clone());
    app.settings()
        .change(&json!({"notifications": {"sound": false}}))
        .unwrap();
    let live = app.events().after(replayed.event_id, 1).unwrap().remove(0);

    let mut body = response.into_body();
    let mut buffered = String::new();
    for notice in [replayed, live] {
        let (id, data) = next_event(&mut body, &mut buffered).await;
        assert_eq!(id, notice.event_id);
        assert_eq!(
            data,
            json!({
                "event_id": notice.event_id,
                "kind": "settings_changed",
                "at": notice.at,
                // the person's settings name no actor or subject yet
                "actor": null,
                "subject": null,
                "review_id": null,
                "review": null,
            })
        );
    }
}

/// A subscriber that falls behind by more than the channel holds has its
/// stream ended, so its EventSource reconnects and catches up, rather than
/// going on with a gap it cannot see.
#[tokio::test]
async fn a_subscriber_that_falls_behind_is_disconnected_not_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(Pinrail::open(Config::new(dir.path(), 0)).unwrap());
    let response = api::router(app.clone())
        .oneshot(Request::get("/api/v1/events").body(Body::empty()).unwrap())
        .await
        .unwrap();
    for n in 0..300 {
        app.settings()
            .change(&json!({"notifications": {"sound": n % 2 == 0}}))
            .unwrap();
    }
    let mut body = response.into_body();
    let ended = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(frame) = body.frame().await {
            frame.unwrap();
        }
    })
    .await;
    assert!(ended.is_ok(), "the stream went on past the notices it lost");
}

/// A browser's EventSource reconnects with the last id it saw in the
/// Last-Event-ID header, and the stream catches up from there.
#[tokio::test]
async fn a_reconnect_catches_up_from_last_event_id() {
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(Pinrail::open(Config::new(dir.path(), 0)).unwrap());
    app.settings().change(&json!({"autostart": true})).unwrap();
    let seen = app.events().after(0, 1).unwrap()[0].event_id;
    app.settings()
        .change(&json!({"appearance": {"theme": "dark"}}))
        .unwrap();
    let missed = app.events().after(seen, 1).unwrap().remove(0);
    let response = api::router(app.clone())
        .oneshot(
            Request::get("/api/v1/events")
                .header("last-event-id", seen.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut body = response.into_body();
    let mut buffered = String::new();
    let (id, _) = next_event(&mut body, &mut buffered).await;
    assert_eq!(id, missed.event_id);
}

/// A client further behind than one page of the backlog gets every event
/// it missed, then the live ones.
#[tokio::test]
async fn a_catch_up_longer_than_a_page_sends_every_missed_event() {
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(Pinrail::open(Config::new(dir.path(), 0)).unwrap());
    app.settings().change(&json!({"autostart": true})).unwrap();
    let seen = app.events().after(0, 1).unwrap()[0].event_id;
    for n in 0..1100 {
        app.settings()
            .change(&json!({"notifications": {"sound": n % 2 == 0}}))
            .unwrap();
    }
    let missed: Vec<i64> = app
        .events()
        .after(seen, 5000)
        .unwrap()
        .iter()
        .map(|n| n.event_id)
        .collect();
    assert!(missed.len() > 1000, "{}", missed.len());
    let response = api::router(app.clone())
        .oneshot(
            Request::get(format!("/api/v1/events?after={seen}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    app.settings()
        .change(&json!({"appearance": {"theme": "dark"}}))
        .unwrap();
    let live = app.events().after(*missed.last().unwrap(), 1).unwrap()[0].event_id;

    let mut body = response.into_body();
    let mut buffered = String::new();
    for expected in missed.iter().chain([&live]) {
        let (id, _) = next_event(&mut body, &mut buffered).await;
        assert_eq!(id, *expected);
    }
}

#[tokio::test]
async fn a_notice_carries_its_time_actor_and_subject_live_and_replayed() {
    use pinrail_core::db::{Db, EventActor, EventSubject, NewEvent};
    use pinrail_core::events::Notice;
    use pinrail_core::items::{Actor, NewItem};
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path(), 0);
    let app = Arc::new(Pinrail::open(config.clone()).unwrap());
    let item = app
        .items()
        .add(
            &NewItem {
                repo: "forgeplane/pinrail".to_string(),
                title: "Work".to_string(),
                body: String::new(),
                parent: None,
                depends_on: Vec::new(),
                link: None,
            },
            None,
            &Actor::Person,
        )
        .unwrap();
    let cursor = app
        .events()
        .after(0, 1000)
        .unwrap()
        .last()
        .unwrap()
        .event_id;

    let response = api::router(app.clone())
        .oneshot(
            Request::get(format!("/api/v1/events?after={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut body = response.into_body();
    let mut buffered = String::new();
    let db = Db::open(&config.db_path()).unwrap();
    let key = item.key.to_string();
    let attrs = json!({ "key": key });
    let event = db
        .append_event_as(&NewEvent {
            kind: "item_changed",
            subject: Some(EventSubject::Item(item.id.clone())),
            actor: EventActor::Person("pnezis".to_string()),
            attrs: &attrs,
        })
        .unwrap();
    app.events()
        .publish(Notice::new(event, "item_changed", &attrs));
    let (id, live) = next_event(&mut body, &mut buffered).await;
    assert_eq!(id, event);
    assert_eq!(
        live["subject"],
        json!({"kind": "item", "id": item.id, "key": key})
    );
    assert_eq!(live["actor"], json!({"kind": "person", "name": "pnezis"}));
    assert!(
        live["at"].as_str().is_some_and(|at| at.ends_with('Z')),
        "{live}"
    );

    // a client that reconnects reads the same
    let response = api::router(app.clone())
        .oneshot(
            Request::get(format!("/api/v1/events?after={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut body = response.into_body();
    let mut buffered = String::new();
    let (id, replayed) = next_event(&mut body, &mut buffered).await;
    assert_eq!(id, event);
    for field in ["at", "actor", "subject"] {
        assert_eq!(replayed[field], live[field], "{field}");
    }
}

#[tokio::test]
async fn a_cursor_older_than_the_stored_events_gets_a_resync() {
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(Pinrail::open(Config::new(dir.path(), 0)).unwrap());
    app.settings().change(&json!({"autostart": true})).unwrap();
    let cursor = app.events().after(0, 1).unwrap()[0].event_id;
    app.settings()
        .change(&json!({"appearance": {"theme": "dark"}}))
        .unwrap();
    app.settings()
        .change(&json!({"notifications": {"sound": false}}))
        .unwrap();
    let newest = app.events().after(0, 100).unwrap().last().unwrap().event_id;
    // the sweep, 15 days on, deletes what the cursor would catch up on
    app.events()
        .sweep(chrono::Utc::now() + chrono::Duration::days(15))
        .unwrap();
    assert!(app.events().after(0, 100).unwrap().is_empty());

    let response = api::router(app.clone())
        .oneshot(
            Request::get(format!("/api/v1/events?after={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut body = response.into_body();
    let mut buffered = String::new();
    let (id, data) = next_event(&mut body, &mut buffered).await;
    assert_eq!(id, newest);
    assert_eq!(data, json!({"event_id": newest, "kind": "resync"}));

    // the live notices follow it
    app.settings()
        .change(&json!({"appearance": {"theme": "light"}}))
        .unwrap();
    let (id, data) = next_event(&mut body, &mut buffered).await;
    assert_eq!(id, newest + 1);
    assert_eq!(data["kind"], "settings_changed");
}

#[tokio::test]
async fn a_cursor_the_stored_events_reach_gets_no_resync() {
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(Pinrail::open(Config::new(dir.path(), 0)).unwrap());
    app.settings().change(&json!({"autostart": true})).unwrap();
    let cursor = app.events().after(0, 1).unwrap()[0].event_id;
    app.settings()
        .change(&json!({"appearance": {"theme": "dark"}}))
        .unwrap();
    let response = api::router(app.clone())
        .oneshot(
            Request::get(format!("/api/v1/events?after={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut body = response.into_body();
    let (id, data) = next_event(&mut body, &mut String::new()).await;
    assert_eq!(id, cursor + 1);
    assert_eq!(data["kind"], "settings_changed");
}
