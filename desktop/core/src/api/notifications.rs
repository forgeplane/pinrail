//! `/api/v1/notifications`: the person's notifications. The lists and the
//! counts are read by the window; every change to the read and done state
//! takes the person token.

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::{Value, json};

use super::ApiState;
use super::error::ApiError;
use super::guard::Person;
use super::parse_body;
use crate::Pinrail;
use crate::db::Listed;
use crate::error::Error;
use crate::notifications::Change;

/// The notifications a page holds when the request does not say.
const PAGE_DEFAULT: usize = 50;

pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/notifications", get(list))
        .route("/api/v1/notifications/counts", get(counts))
        .route("/api/v1/notifications/read", post(read_all))
        .route("/api/v1/notifications/{id}/read", put(read).delete(unread))
        .route("/api/v1/notifications/{id}/done", put(done).delete(undone))
}

/// The list a `tab` names: `waiting`, `unread`, or `all`, the default.
fn tab(value: Option<&str>) -> Result<Listed, Error> {
    match value.unwrap_or("all") {
        "waiting" => Ok(Listed::Waiting),
        "unread" => Ok(Listed::Unread),
        "all" | "" => Ok(Listed::Open),
        _ => Err(Error::invalid(
            "tab",
            "must be one of waiting, unread or all",
        )),
    }
}

/// A count from the query, or `default` when it is absent.
fn count(params: &HashMap<String, String>, name: &str, default: usize) -> Result<usize, Error> {
    params.get(name).map_or(Ok(default), |v| {
        v.parse()
            .map_err(|_| Error::invalid(name, "must be a whole number"))
    })
}

/// A page of a list, newest first: `tab`, `limit` and `offset`. Answers
/// the notifications, how many the list holds, the counts, and `up_to`, the
/// newest event the list reflects.
async fn list(
    State(state): State<Arc<Pinrail>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let listed = tab(params.get("tab").map(String::as_str))?;
    let limit = count(&params, "limit", PAGE_DEFAULT)?;
    let offset = count(&params, "offset", 0)?;
    Ok(Json(state.notifications().list(listed, limit, offset)?))
}

/// How many notifications wait for the person, and how many are unread.
async fn counts(State(state): State<Arc<Pinrail>>) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.notifications().counts()?))
}

/// Marks read what a list showed: `tab`, and `up_to`, the newest event
/// the list reflected. Answers how many were marked.
async fn read_all(
    _person: Person,
    State(state): State<Arc<Pinrail>>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = parse_body(&body)?;
    let listed = tab(body.get("tab").and_then(Value::as_str))?;
    let up_to = body
        .get("up_to")
        .and_then(Value::as_i64)
        .ok_or_else(|| Error::invalid("/up_to", "give the newest event id the list showed"))?;
    let read = state.notifications().read_all(listed, up_to)?;
    Ok(Json(json!({ "read": read })))
}

fn change(state: &Pinrail, id: i64, change: Change) -> Result<StatusCode, ApiError> {
    state.notifications().change(id, change)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn read(
    _person: Person,
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    change(&state, id, Change::Read)
}

async fn unread(
    _person: Person,
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    change(&state, id, Change::Unread)
}

async fn done(
    _person: Person,
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    change(&state, id, Change::Done)
}

async fn undone(
    _person: Person,
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    change(&state, id, Change::Undone)
}
