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

use super::ApiState;
use crate::Wicket;
use crate::events::Notice;

const CATCH_UP_LIMIT: usize = 1000;

pub fn routes() -> Router<ApiState> {
    Router::new().route("/api/v1/events", get(events))
}

async fn events(
    State(state): State<Arc<Wicket>>,
    Query(params): Query<HashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Subscribe before reading the backlog, so nothing between the two is lost.
    let rx = state.events().subscribe();
    let after = params.get("after").and_then(|a| a.parse::<i64>().ok());
    let backlog: Vec<Notice> = match after {
        Some(after) => state
            .events()
            .after(after, CATCH_UP_LIMIT)
            .unwrap_or_default(),
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
