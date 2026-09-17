//! `/api/v1/settings`: every setting, and a partial change to them.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::Value;

use super::parse_body;
use crate::Wicket;
use crate::error::Error;

pub fn routes() -> Router<Arc<Wicket>> {
    Router::new().route("/api/v1/settings", get(show).patch(change))
}

async fn show(State(state): State<Arc<Wicket>>) -> Json<Value> {
    Json(state.settings.get())
}

async fn change(State(state): State<Arc<Wicket>>, body: Bytes) -> Response {
    match apply(&state, &body) {
        Ok(value) => Json(value).into_response(),
        Err(error) => error.into_response(),
    }
}

fn apply(state: &Wicket, body: &[u8]) -> Result<Value, Error> {
    state.change_settings(&parse_body(body)?)
}
