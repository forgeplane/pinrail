//! `/api/v1/reviews`: submit, read, list, rounds, long-poll wait, decide,
//! withdraw, discard, events.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use super::ApiState;
use super::error::ApiError;
use super::parse_body;
use crate::Wicket;
use crate::error::Error;
use crate::reviews::{Filters, Status};

const DEFAULT_WAIT: u64 = 300;
const MAX_WAIT: u64 = 600;

pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/reviews", post(submit).get(list))
        .route("/api/v1/reviews/{id}", get(show))
        .route("/api/v1/reviews/{id}/rounds", get(rounds))
        .route("/api/v1/reviews/{id}/wait", get(wait))
        .route("/api/v1/reviews/{id}/decision", post(decide))
        .route("/api/v1/reviews/{id}/withdraw", post(withdraw))
        .route("/api/v1/reviews/{id}/discard", post(discard))
        .route("/api/v1/reviews/{id}/viewed", post(viewed))
        .route("/api/v1/reviews/{id}/events", get(events))
}

async fn submit(State(state): State<Arc<Wicket>>, body: Bytes) -> Result<Response, ApiError> {
    let body = parse_body(&body)?;
    let review = state.reviews().submit(&body, None)?;
    Ok((StatusCode::CREATED, Json(review.to_json(true))).into_response())
}

/// A page of reviews, newest first, without payloads, in an envelope:
/// `reviews`, `total`, `has_more` and `next_cursor`, and `facets` with
/// `include=facets`. Walk everything with `cursor=<next_cursor>`; number
/// pages with `offset`.
async fn list(
    State(state): State<Arc<Wicket>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let filters = filters(&params)?;
    let mut facets = false;
    if let Some(include) = params.get("include") {
        for name in include.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            match name {
                "facets" => facets = true,
                other => {
                    return Err(
                        Error::invalid("/include", format!("unknown include {other}")).into(),
                    );
                }
            }
        }
    }
    let listing = state.reviews().listing(&filters, facets)?;
    let mut body = json!({
        "reviews": listing.reviews.iter().map(|r| r.to_json(false)).collect::<Vec<_>>(),
        "total": listing.total,
        "has_more": listing.has_more,
        "next_cursor": listing.next_cursor,
    });
    if let Some(f) = listing.facets {
        body["facets"] =
            json!({ "plugins": f.plugins, "repos": f.repos, "unassigned": f.unassigned });
    }
    Ok(Json(body))
}

/// Whether the caller wants the review as markdown: `?format=markdown`,
/// or an Accept header that names text/markdown ahead of JSON.
fn wants_markdown(headers: &HeaderMap, params: &HashMap<String, String>) -> bool {
    if let Some(f) = params.get("format") {
        return f == "markdown" || f == "md";
    }
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| {
            let md = accept.find("text/markdown");
            let json = accept.find("application/json");
            matches!((md, json), (Some(m), Some(j)) if m < j)
                || matches!((md, json), (Some(_), None))
        })
}

/// A review as the caller asked for it: markdown with its round placed
/// in the chain, or the JSON everything else reads.
fn review_response(
    state: &Wicket,
    review: &crate::reviews::Review,
    markdown: bool,
) -> Result<Response, ApiError> {
    if !markdown {
        return Ok(Json(review.to_json(true)).into_response());
    }
    let rounds = state.reviews().rounds(&review.id)?;
    let round = (rounds.len() > 1)
        .then(|| {
            rounds
                .iter()
                .position(|r| r.id == review.id)
                .map(|i| (i + 1, rounds.len()))
        })
        .flatten();
    let template = state
        .plugins()
        .fetch_version(&review.plugin, review.plugin_version)
        .ok()
        .and_then(|p| p.decision_template.clone());
    let text = crate::markdown::render(&review.to_json(true), round, template.as_deref());
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        text,
    )
        .into_response())
}

async fn show(
    State(state): State<Arc<Wicket>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let review = state.reviews().get(&id)?;
    review_response(&state, &review, wants_markdown(&headers, &params))
}

async fn rounds(
    State(state): State<Arc<Wicket>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let rounds = state.reviews().rounds(&id)?;
    Ok(Json(Value::Array(
        rounds.iter().map(|r| r.to_json(false)).collect(),
    )))
}

async fn wait(
    State(state): State<Arc<Wicket>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let timeout = params
        .get("timeout")
        .and_then(|t| t.parse::<u64>().ok())
        .unwrap_or(DEFAULT_WAIT)
        .min(MAX_WAIT);
    match state
        .reviews()
        .wait(&id, Duration::from_secs(timeout))
        .await?
    {
        Some(review) => review_response(&state, &review, wants_markdown(&headers, &params)),
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}

async fn decide(
    State(state): State<Arc<Wicket>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let body = parse_body(&body)?;
    let Some(data) = body.get("data") else {
        return Err(Error::invalid("/data", "is required").into());
    };
    let note = body.get("agent_note").and_then(Value::as_str);
    let review = state.reviews().decide(&id, data, note)?;
    review_response(&state, &review, wants_markdown(&headers, &params))
}

async fn withdraw(
    State(state): State<Arc<Wicket>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = parse_body(&body)?;
    let reason = body.get("reason").and_then(Value::as_str);
    Ok(Json(state.reviews().withdraw(&id, reason)?.to_json(true)))
}

async fn discard(
    State(state): State<Arc<Wicket>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = if body.is_empty() {
        json!({})
    } else {
        parse_body(&body)?
    };
    let reason = body.get("reason").and_then(Value::as_str);
    let by = body.get("by").and_then(Value::as_str);
    Ok(Json(
        state.reviews().discard(&id, by, reason)?.to_json(true),
    ))
}

async fn viewed(
    State(state): State<Arc<Wicket>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state.reviews().mark_viewed(&id)?;
    Ok(Json(json!({ "ok": true })))
}

async fn events(
    State(state): State<Arc<Wicket>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let events = state.reviews().events(&id)?;
    Ok(Json(Value::Array(
        events.iter().map(|e| e.to_json()).collect(),
    )))
}

fn filters(params: &HashMap<String, String>) -> Result<Filters, ApiError> {
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
            )
            .into());
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
        offset: params
            .get("offset")
            .and_then(|o| o.parse().ok())
            .unwrap_or(0),
    })
}
