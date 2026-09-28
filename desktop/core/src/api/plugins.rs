//! `/api/v1/plugins`: translate HTTP requests into plugin service operations.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use super::ApiState;
use super::error::ApiError;
use super::parse_body;
use crate::Pinrail;
use crate::error::Error;
use crate::plugins::{InstallOptions, UpdateOutcome};

pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/plugins", get(index))
        .route("/api/v1/plugins/describe", get(describe_all))
        .route("/api/v1/plugins/reload", post(reload))
        .route("/api/v1/plugins/inspect", post(inspect))
        .route("/api/v1/plugins/check", post(check))
        .route("/api/v1/plugins/install", post(install))
        .route("/api/v1/plugins/jobs/{id}", get(job))
        .route("/api/v1/plugins/{name}/updates", get(updates))
        .route("/api/v1/plugins/{name}/update", post(update))
        .route("/api/v1/plugins/{name}", delete(remove))
        .route("/api/v1/plugins/{name}/versions", get(versions))
        .route("/api/v1/plugins/{name}/describe", get(describe))
        .route("/api/v1/plugins/{name}/sample", post(sample))
}

async fn index(State(state): State<Arc<Pinrail>>) -> Json<Value> {
    let stored = state.settings().value(crate::settings::PLUGINS);
    Json(state.plugins().listing(&stored))
}

/// Sends the plugin's sample as a new review; the body may give a
/// `title`, `requested_by` or `origin`. 201 with the review.
async fn sample(
    State(state): State<Arc<Pinrail>>,
    Path(name): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let overrides = if body.is_empty() {
        json!({})
    } else {
        parse_body(&body)?
    };
    let review = state.send_sample(&name, &overrides)?;
    Ok((StatusCode::CREATED, Json(review.to_json(true))).into_response())
}

async fn describe_all(State(state): State<Arc<Pinrail>>) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.plugins().describe(None)?))
}

async fn describe(
    State(state): State<Arc<Pinrail>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.plugins().describe(Some(&name))?))
}

async fn versions(
    State(state): State<Arc<Pinrail>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.plugins().versions(&name)?))
}

/// The source and the options an install or an inspect takes:
/// `{source, link?, force?, ref?, path?}`.
fn install_request(body: &Bytes) -> Result<(String, InstallOptions), Error> {
    let body = parse_body(body)?;
    let Some(source) = body.get("source").and_then(Value::as_str) else {
        return Err(Error::invalid("/source", "is required"));
    };
    let options = InstallOptions {
        link: body.get("link").and_then(Value::as_bool).unwrap_or(false),
        force: body.get("force").and_then(Value::as_bool).unwrap_or(false),
        reference: body.get("ref").and_then(Value::as_str).map(str::to_string),
        path: body.get("path").and_then(Value::as_str).map(str::to_string),
    };
    Ok((source.to_string(), options))
}

async fn inspect(State(state): State<Arc<Pinrail>>, body: Bytes) -> Result<Json<Value>, ApiError> {
    let (source, options) = install_request(&body)?;
    Ok(Json(state.plugins().inspect(&source, options).await?))
}

/// What the app makes of a plugin folder on this machine, installing
/// nothing: `{"dir": "/abs/path"}`, answered with the loader's verdict.
async fn check(body: Bytes) -> Result<Json<Value>, ApiError> {
    let body = parse_body(&body)?;
    let dir = body
        .get("dir")
        .and_then(Value::as_str)
        .map(std::path::PathBuf::from)
        .filter(|d| d.is_absolute())
        .ok_or_else(|| Error::invalid("/dir", "must be an absolute path"))?;
    if !dir.is_dir() {
        return Err(Error::invalid("/dir", format!("{} is not a folder", dir.display())).into());
    }
    Ok(Json(crate::plugins::Plugin::check(&dir)))
}

async fn install(State(state): State<Arc<Pinrail>>, body: Bytes) -> Result<Response, ApiError> {
    let (source, options) = install_request(&body)?;
    let id = state.plugins().start_install(&source, options);
    Ok((StatusCode::ACCEPTED, Json(json!({ "job": id }))).into_response())
}

async fn updates(
    State(state): State<Arc<Pinrail>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.plugins().check_updates(&name).await?))
}

async fn update(
    State(state): State<Arc<Pinrail>>,
    Path(name): Path<String>,
) -> Result<Response, ApiError> {
    match state.plugins().start_update(&name).await? {
        UpdateOutcome::UpToDate { version } => {
            Ok(Json(json!({ "state": "up_to_date", "version": version })).into_response())
        }
        UpdateOutcome::Started { job_id } => Ok((
            StatusCode::ACCEPTED,
            Json(json!({ "job": job_id, "state": "updating" })),
        )
            .into_response()),
    }
}

/// Removes an installed plugin: the record and the store entries no
/// review renders from; the ones a review still uses stay, and the answer
/// names them. A built-in has no record and cannot be removed.
async fn remove(
    State(state): State<Arc<Pinrail>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let answer = state.plugins().remove(&name)?;
    // the links it was allowed to open go with it
    if state
        .settings()
        .value(&format!("/links/{name}"))
        .is_object()
    {
        state
            .settings()
            .change(&serde_json::json!({ "links": { name: null } }))?;
    }
    Ok(Json(answer))
}

/// An install job as it stands: its step, its log so far, and how it ended.
async fn job(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.plugins().job(&id)?.to_json()))
}

async fn reload(State(state): State<Arc<Pinrail>>) -> Result<Json<Value>, ApiError> {
    let count = state.plugins().reload()?;
    Ok(Json(json!({ "ok": true, "count": count })))
}
