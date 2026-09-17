//! `/api/v1/events`: server-sent events for the shell and the tray. Each
//! event carries its database id, so a client that reconnects passes
//! `after=<id>` and misses nothing.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use futures_util::StreamExt;
use futures_util::stream::{self, Stream};
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;

use crate::Wicket;
use serde_json::Value;

use crate::events::Notice;

const CATCH_UP_LIMIT: usize = 1000;

pub fn routes() -> Router<Arc<Wicket>> {
    Router::new().route("/api/v1/events", get(events))
}

async fn events(
    State(state): State<Arc<Wicket>>,
    Query(params): Query<HashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Subscribe before reading the backlog, so nothing between the two is lost.
    let rx = state.reviews.bus().subscribe();
    let after = params.get("after").and_then(|a| a.parse::<i64>().ok());
    let backlog: Vec<Notice> = match after {
        Some(after) => state
            .db
            .events_after(after, CATCH_UP_LIMIT)
            .unwrap_or_default()
            .into_iter()
            .map(|e| Notice {
                event_id: e.id,
                kind: e.kind,
                review: e
                    .review_id
                    .as_deref()
                    .and_then(|id| state.db.get_review(id).ok().flatten())
                    .map(|r| r.to_json(false)),
                review_id: e.review_id,
                keys: e.attrs.get("keys").and_then(Value::as_array).map(|k| {
                    k.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                }),
            })
            .collect(),
        None => Vec::new(),
    };
    let last_backlog_id = backlog.last().map(|n| n.event_id).or(after).unwrap_or(0);

    let live = BroadcastStream::new(rx)
        .filter_map(|item| async move { item.ok() })
        .filter(move |n| {
            let fresh = n.event_id > last_backlog_id;
            async move { fresh }
        });
    let all = stream::iter(backlog).chain(live).map(|n| {
        Ok(Event::default()
            .id(n.event_id.to_string())
            .event(n.kind.clone())
            .json_data(json!({
                "event_id": n.event_id,
                "kind": n.kind,
                "review_id": n.review_id,
                "review": n.review,
            }))
            .unwrap_or_else(|_| Event::default()))
    });
    Sse::new(all).keep_alive(KeepAlive::default())
}
