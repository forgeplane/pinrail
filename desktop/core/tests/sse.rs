//! Catch-up and live notices through the SSE transport.

use std::sync::Arc;

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use wicket_core::{Config, Wicket, api};

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
    let app = Arc::new(Wicket::open(Config::new(dir.path(), 0)).unwrap());
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
                "review_id": null,
                "review": null,
            })
        );
    }
}
