//! `/api/v1/settings`: every setting, and a partial change to them.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::Value;

use super::{AppState, parse_body};
use crate::error::Error;
use crate::events;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/settings", get(show).patch(change))
}

async fn show(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(state.settings.get())
}

async fn change(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    match apply(&state, &body) {
        Ok(value) => Json(value).into_response(),
        Err(error) => error.into_response(),
    }
}

fn apply(state: &AppState, body: &[u8]) -> Result<Value, Error> {
    state.change_settings(&parse_body(body)?)
}

/// Records and broadcasts which settings changed, from a patch or from an
/// edit to the file.
pub fn announce(state: &AppState, keys: &[String]) -> Result<(), Error> {
    let event_id = state.db.append_event(
        None,
        events::SETTINGS_CHANGED,
        None,
        &serde_json::json!({ "keys": keys }),
    )?;
    state
        .reviews
        .publish_keys(event_id, events::SETTINGS_CHANGED, keys.to_vec());
    Ok(())
}
