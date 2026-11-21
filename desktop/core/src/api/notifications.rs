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
use crate::db::{Listed, NotificationFilters, SUBJECT_KINDS};
use crate::error::Error;
use crate::notifications::Change;

/// The notifications a page holds when the request does not say.
const PAGE_DEFAULT: usize = 50;

pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/notifications", get(list))
        .route("/api/v1/notifications/counts", get(counts))
        .route("/api/v1/notifications/read", post(read_all))
        .route("/api/v1/notifications/{id}/events", get(events))
        .route("/api/v1/notifications/{id}/read", put(read).delete(unread))
        .route("/api/v1/notifications/{id}/done", put(done).delete(undone))
}

/// The list a `tab` names: `waiting`, `unread`, or `all`, the default.
fn tab(value: Option<&str>) -> Result<Listed, Error> {
    match value.unwrap_or("all") {
        "waiting" => Ok(Listed::Waiting),
        "unread" => Ok(Listed::Unread),
        "updates" => Ok(Listed::Updates),
        "all" | "" => Ok(Listed::Open),
        _ => Err(Error::invalid(
            "tab",
            "must be one of waiting, unread, updates or all",
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
    let filters = filters(|name| params.get(name).cloned())?;
    let limit = count(&params, "limit", PAGE_DEFAULT)?;
    let offset = count(&params, "offset", 0)?;
    Ok(Json(state.notifications().list(&filters, limit, offset)?))
}

/// The tab and the filters of a list, from the query or the body: `tab`,
/// `kind`, `scope` and `plugin`, each of the last three a comma-separated
/// list of alternatives, `q`, and `done`.
fn filters(get: impl Fn(&str) -> Option<String>) -> Result<NotificationFilters, Error> {
    let list = |name: &str| -> Vec<String> {
        get(name)
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .collect()
    };
    let kinds = list("kind");
    if let Some(unknown) = kinds.iter().find(|k| {
        !crate::notifications::REASONS.contains(&k.as_str()) && !SUBJECT_KINDS.contains(&k.as_str())
    }) {
        return Err(Error::invalid(
            "kind",
            format!(
                "{unknown} is no kind: give a reason, {}, or a subject, {}",
                crate::notifications::REASONS.join(", "),
                SUBJECT_KINDS.join(", ")
            ),
        ));
    }
    Ok(NotificationFilters {
        listed: tab(get("tab").as_deref())?,
        kinds,
        scopes: list("scope"),
        plugins: list("plugin"),
        q: get("q").filter(|q| !q.trim().is_empty()),
        done: get("done").is_some_and(|v| v == "true" || v == "1"),
    })
}

/// The events of a notification's subject after `after`, oldest first, at
/// most `limit`, as the event stream sends them.
async fn events(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<i64>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let after = params
        .get("after")
        .map_or(Ok(0), |v| v.parse::<i64>())
        .map_err(|_| Error::invalid("after", "must be an event id"))?;
    let limit = count(&params, "limit", PAGE_DEFAULT)?;
    let events = state.notifications().events(id, after, limit)?;
    Ok(Json(json!({ "events": events })))
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
    let filters = filters(|name| match body.get(name) {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Bool(on)) => Some(on.to_string()),
        _ => None,
    })?;
    let up_to = body
        .get("up_to")
        .and_then(Value::as_i64)
        .ok_or_else(|| Error::invalid("/up_to", "give the newest event id the list showed"))?;
    let read = state.notifications().read_all(&filters, up_to)?;
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
