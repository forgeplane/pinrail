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
use crate::plugins::InstallOptions;

pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/plugins", get(index))
        .route("/api/v1/plugins/describe", get(describe_all))
        .route("/api/v1/plugins/catalog", get(catalog))
        .route("/api/v1/plugins/inspect", post(inspect))
        .route("/api/v1/plugins/install", post(install))
        .route("/api/v1/plugins/{name}", delete(remove))
        .route("/api/v1/plugins/{name}/describe", get(describe))
        .route("/api/v1/plugins/{name}/sample", post(sample))
}

async fn index(State(state): State<Arc<Pinrail>>) -> Json<Value> {
    let stored = state.settings().value(crate::settings::PLUGINS);
    Json(state.plugins().listing(&stored))
}

/// Sends one of the plugin's samples as a new review; the body may name
/// the `sample` (the first otherwise) and give a `title`, `requested_by`
/// or `origin`. 201 with the review.
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

/// The official plugins the app can install, in the shape of the
/// registry's compiled index.
async fn catalog(State(state): State<Arc<Pinrail>>) -> Json<Value> {
    Json(state.plugins().catalog())
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

/// The source and the options an install or an inspect takes:
/// `{source, link?}`.
fn install_request(body: &Bytes) -> Result<(String, InstallOptions), Error> {
    let body = parse_body(body)?;
    let Some(source) = body.get("source").and_then(Value::as_str) else {
        return Err(Error::invalid("/source", "is required"));
    };
    let options = InstallOptions {
        link: body.get("link").and_then(Value::as_bool).unwrap_or(false),
    };
    Ok((source.to_string(), options))
}

async fn inspect(State(state): State<Arc<Pinrail>>, body: Bytes) -> Result<Json<Value>, ApiError> {
    let (source, options) = install_request(&body)?;
    Ok(Json(state.plugins().inspect(&source, options).await?))
}

/// Installs the plugin a folder or a zip holds, `{source, link?}`, or the
/// official plugin an id names, `{id, version?}`, at that version or the
/// highest offered; the plugin's row.
async fn install(State(state): State<Arc<Pinrail>>, body: Bytes) -> Result<Json<Value>, ApiError> {
    let request = parse_body(&body)?;
    if let Some(id) = request.get("id") {
        let id = id
            .as_str()
            .ok_or_else(|| Error::invalid("/id", "is not a string"))?;
        let version = match request.get("version") {
            None | Some(Value::Null) => None,
            Some(Value::String(version)) => Some(version.as_str()),
            Some(_) => return Err(Error::invalid("/version", "is not a string").into()),
        };
        return Ok(Json(state.plugins().install_offered_at(id, version).await?));
    }
    let (source, options) = install_request(&body)?;
    Ok(Json(state.plugins().install(&source, options).await?))
}

/// Removes an installed plugin; the reviews made with it keep the bundles
/// they were submitted to.
async fn remove(
    State(state): State<Arc<Pinrail>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let answer = state.plugins().remove(&name)?;
    // the links it was allowed to open go with it
    if state
        .settings()
        .value(&format!(
            "{}/{}",
            crate::settings::LINKS,
            crate::settings::pointer_part(&name)
        ))
        .is_object()
    {
        state
            .settings()
            .change(&serde_json::json!({ "links": { &name: null } }))?;
    }
    Ok(Json(answer))
}
