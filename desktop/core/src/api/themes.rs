//! `/api/v1/themes`: every theme, and a new theme copied from another.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};

use super::ApiState;
use super::error::ApiError;
use super::parse_body;
use crate::Pinrail;
use crate::error::Error;

pub fn routes() -> Router<ApiState> {
    Router::new().route("/api/v1/themes", get(index).post(create))
}

async fn index(State(state): State<Arc<Pinrail>>) -> Json<Value> {
    Json(json!({ "themes": state.themes().list() }))
}

async fn create(State(state): State<Arc<Pinrail>>, body: Bytes) -> Response {
    match copy(&state, &body) {
        Ok(path) => (
            StatusCode::CREATED,
            Json(json!({ "path": path.to_string_lossy() })),
        )
            .into_response(),
        Err(error) => ApiError(error).into_response(),
    }
}

fn copy(state: &Pinrail, body: &[u8]) -> Result<std::path::PathBuf, Error> {
    let request: Value = parse_body(body)?;
    let text = |key: &str| {
        request[key]
            .as_str()
            .ok_or_else(|| Error::invalid(format!("/{key}"), "must be a string"))
    };
    state.themes().create(text("id")?, text("from")?)
}
