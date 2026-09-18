//! `/api/v1/plugins`: translate HTTP requests into plugin service operations.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use super::error::ApiError;
use super::parse_body;
use crate::Wicket;
use crate::error::Error;
use crate::plugins::{InstallOptions, UpdateOutcome};

pub fn routes() -> Router<Arc<Wicket>> {
    Router::new()
        .route("/api/v1/plugins", get(index))
        .route("/api/v1/plugins/reload", post(reload))
        .route("/api/v1/plugins/inspect", post(inspect))
        .route("/api/v1/plugins/install", post(install))
        .route("/api/v1/plugins/jobs/{id}", get(job))
        .route("/api/v1/plugins/{name}/updates", get(updates))
        .route("/api/v1/plugins/{name}/update", post(update))
        .route("/api/v1/plugins/{name}", delete(remove))
        .route("/api/v1/plugins/{name}/versions", get(versions))
}

async fn index(State(state): State<Arc<Wicket>>) -> Json<Value> {
    let stored = state.settings().value(crate::settings::PLUGINS);
    Json(state.plugins().listing(&stored))
}

async fn versions(
    State(state): State<Arc<Wicket>>,
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

async fn inspect(State(state): State<Arc<Wicket>>, body: Bytes) -> Result<Json<Value>, ApiError> {
    let (source, options) = install_request(&body)?;
    Ok(Json(state.plugins().inspect(&source, options).await?))
}

async fn install(State(state): State<Arc<Wicket>>, body: Bytes) -> Result<Response, ApiError> {
    let (source, options) = install_request(&body)?;
    let id = state.plugins().start_install(&source, options);
    Ok((StatusCode::ACCEPTED, Json(json!({ "job": id }))).into_response())
}

async fn updates(
    State(state): State<Arc<Wicket>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.plugins().check_updates(&name).await?))
}

async fn update(
    State(state): State<Arc<Wicket>>,
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
    State(state): State<Arc<Wicket>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.plugins().remove(&name)?))
}

/// An install job as it stands: its step, its log so far, and how it ended.
async fn job(
    State(state): State<Arc<Wicket>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.plugins().job(&id)?.to_json()))
}

async fn reload(State(state): State<Arc<Wicket>>) -> Result<Json<Value>, ApiError> {
    let count = state.plugins().reload()?;
    Ok(Json(json!({ "ok": true, "count": count })))
}
