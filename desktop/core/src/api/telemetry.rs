//! `/api/v1/telemetry`: where usage data stands, the events that wait to
//! be sent and the last ones sent. The switches are settings, changed with
//! `/api/v1/settings`.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::Value;

use super::ApiState;
use crate::Pinrail;

pub fn routes() -> Router<ApiState> {
    Router::new().route("/api/v1/telemetry", get(show))
}

async fn show(State(state): State<Arc<Pinrail>>) -> Json<Value> {
    Json(state.telemetry().status())
}
