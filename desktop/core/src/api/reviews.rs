//! `/api/v1/reviews`: submit, read, list, rounds, long-poll wait, decide,
//! withdraw, events.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use super::{AppState, parse_body};
use crate::db::Filters;
use crate::error::Error;
use crate::review::Status;

const DEFAULT_WAIT: u64 = 300;
const MAX_WAIT: u64 = 600;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/reviews", post(submit).get(list))
        .route("/api/v1/reviews/{id}", get(show))
        .route("/api/v1/reviews/{id}/rounds", get(rounds))
        .route("/api/v1/reviews/{id}/wait", get(wait))
        .route("/api/v1/reviews/{id}/decision", post(decide))
        .route("/api/v1/reviews/{id}/withdraw", post(withdraw))
        .route("/api/v1/reviews/{id}/viewed", post(viewed))
        .route("/api/v1/reviews/{id}/events", get(events))
}

async fn submit(State(state): State<Arc<AppState>>, body: Bytes) -> Result<Response, Error> {
    let body = parse_body(&body)?;
    let review = state.reviews.submit(&body, None)?;
    Ok((StatusCode::CREATED, Json(review.to_json(true))).into_response())
}

async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, Error> {
    let filters = filters(&params)?;
    let reviews = state.reviews.list(&filters)?;
    Ok(Json(Value::Array(
        reviews.iter().map(|r| r.to_json(false)).collect(),
    )))
}

async fn show(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    Ok(Json(state.reviews.get(&id)?.to_json(true)))
}

async fn rounds(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    let rounds = state.reviews.rounds(&id)?;
    Ok(Json(Value::Array(
        rounds.iter().map(|r| r.to_json(false)).collect(),
    )))
}

async fn wait(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, Error> {
    let timeout = params
        .get("timeout")
        .and_then(|t| t.parse::<u64>().ok())
        .unwrap_or(DEFAULT_WAIT)
        .min(MAX_WAIT);
    match state
        .reviews
        .wait(&id, Duration::from_secs(timeout))
        .await?
    {
        Some(review) => Ok(Json(review.to_json(true)).into_response()),
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}

async fn decide(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, Error> {
    let body = parse_body(&body)?;
    let Some(data) = body.get("data") else {
        return Err(Error::invalid("/data", "is required"));
    };
    let note = body.get("agent_note").and_then(Value::as_str);
    Ok(Json(state.reviews.decide(&id, data, note)?.to_json(true)))
}

async fn withdraw(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, Error> {
    let body = parse_body(&body)?;
    let reason = body.get("reason").and_then(Value::as_str);
    Ok(Json(state.reviews.withdraw(&id, reason)?.to_json(true)))
}

async fn viewed(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    state.reviews.mark_viewed(&id)?;
    Ok(Json(json!({ "ok": true })))
}

async fn events(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    let events = state.reviews.events(&id)?;
    Ok(Json(Value::Array(
        events.iter().map(|e| e.to_json()).collect(),
    )))
}

fn filters(params: &HashMap<String, String>) -> Result<Filters, Error> {
    let mut statuses = Vec::new();
    if let Some(s) = params.get("status") {
        let mut unknown = Vec::new();
        for name in s.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            match Status::parse(name) {
                Some(status) => statuses.push(status),
                None => unknown.push(name.to_string()),
            }
        }
        if !unknown.is_empty() {
            return Err(Error::invalid(
                "/status",
                format!("unknown status {}", unknown.join(", ")),
            ));
        }
    }
    let text = |key: &str| params.get(key).filter(|v| !v.is_empty()).cloned();
    Ok(Filters {
        statuses,
        plugin: text("plugin"),
        repo: text("repo"),
        workflow: text("workflow"),
        reference: text("ref"),
        run_id: text("run_id"),
        text: text("q"),
        include_revised: params.get("include_revised").is_some_and(|v| v == "true"),
        cursor: text("cursor"),
        limit: params
            .get("limit")
            .and_then(|l| l.parse().ok())
            .unwrap_or(0),
    })
}
