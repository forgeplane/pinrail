//! `/api/v1/events`: server-sent events for the shell and the tray. Each
//! event carries its database id, so a client that reconnects catches up
//! from the last one it saw: by `after=<id>`, or by the Last-Event-ID
//! header a browser's EventSource sends. The catch-up reads the stored
//! events a page at a time, up to the newest, before the live ones. A
//! client that falls so far behind that notices are lost, or whose
//! catch-up cannot be read, is disconnected, so that it reconnects and
//! catches up rather than going on with a gap.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use axum::Router;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use futures_util::StreamExt;
use futures_util::stream::{self, Stream};
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;

use super::ApiState;
use crate::Pinrail;

/// The stored events read at a time for a catch-up.
const CATCH_UP_PAGE: usize = 1000;

pub fn routes() -> Router<ApiState> {
    Router::new().route("/api/v1/events", get(events))
}

async fn events(
    State(state): State<Arc<Pinrail>>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Subscribe before reading the backlog, so nothing between the two is lost.
    let rx = state.events().subscribe();
    let after = params
        .get("after")
        .map(String::as_str)
        .or_else(|| headers.get("last-event-id").and_then(|v| v.to_str().ok()))
        .and_then(|a| a.trim().parse::<i64>().ok());
    // the last id sent from the backlog, and whether reading it failed:
    // the live notices start after the one, and do not start after the other
    let last_sent = Arc::new(AtomicI64::new(after.unwrap_or(0)));
    let failed = Arc::new(AtomicBool::new(false));
    let backlog = stream::unfold(after, {
        let (state, last_sent, failed) = (state.clone(), last_sent.clone(), failed.clone());
        move |cursor| {
            let (state, last_sent, failed) = (state.clone(), last_sent.clone(), failed.clone());
            async move {
                let page = match state.events().after(cursor?, CATCH_UP_PAGE) {
                    Ok(page) => page,
                    Err(error) => {
                        eprintln!("pinrail: the events to catch up on could not be read: {error}");
                        failed.store(true, Ordering::SeqCst);
                        return None;
                    }
                };
                let last = page.last()?.event_id;
                last_sent.store(last, Ordering::SeqCst);
                // a full page may have more after it
                let next = (page.len() == CATCH_UP_PAGE).then_some(last);
                Some((stream::iter(page), next))
            }
        }
    })
    .flatten();

    let live = BroadcastStream::new(rx)
        // lagged: notices were dropped for this subscriber, so the stream
        // ends and the client reconnects from its last id
        .take_while(|item| futures_util::future::ready(item.is_ok()))
        .filter_map(|item| async move { item.ok() })
        .filter(move |n| {
            let fresh = n.event_id > last_sent.load(Ordering::SeqCst);
            async move { fresh }
        });
    // none after a catch-up that failed: the stream ends there
    let live = stream::once(async move { (!failed.load(Ordering::SeqCst)).then_some(live) })
        .filter_map(futures_util::future::ready)
        .flatten();
    let all = backlog.chain(live).map(|n| {
        let mut data = json!({
            "event_id": n.event_id,
            "kind": n.kind,
            "review_id": n.review_id,
            "review": n.review,
        });
        // a session's status, on the notice that brings one
        if let Some(session) = &n.session {
            data["session"] = session.clone();
        }
        // the item, and who changed it, on an item's notice
        if let Some(item) = &n.item {
            data["item"] = item.clone();
        }
        Ok(Event::default()
            .id(n.event_id.to_string())
            .event(n.kind.clone())
            .json_data(data)
            .unwrap_or_else(|_| Event::default()))
    });
    Sse::new(all).keep_alive(KeepAlive::default())
}
