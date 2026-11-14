//! The agents Pinrail starts: start one in a folder, list them, read one
//! with the sessions it has been in, and stop one. Starting and stopping
//! are the person's.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use super::ApiState;
use super::error::ApiError;
use super::guard::{Person, refuse};
use crate::Pinrail;
use crate::db::{RunFilters, RunRow};
use crate::error::Error;
use crate::runtime::{LOCAL, RunError, Start};

pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/runs", post(start).get(list))
        .route("/api/v1/runs/{run}", get(show))
        .route("/api/v1/runs/{run}/stop", post(stop))
}

/// Starts `agent` in `folder`, with an optional first `task`, extra `args`
/// and the terminal's `cols` and `rows`. `host` is `local`, the only host
/// so far. A `local_id` makes the call idempotent for 10 minutes. Answers
/// 201 with the run and its session, which is null for an agent that
/// names its session only once it starts.
async fn start(
    State(state): State<Arc<Pinrail>>,
    _person: Person,
    Json(body): Json<Value>,
) -> Response {
    let request = match parse_start(&body) {
        Ok(request) => request,
        Err(error) => return run_error(error),
    };
    let local_id = body.get("local_id").and_then(Value::as_str);
    match state.runtime().start_once(local_id, &request).await {
        Ok(run) => (StatusCode::CREATED, Json(started(&run))).into_response(),
        Err(error) => run_error(error),
    }
}

fn parse_start(body: &Value) -> Result<Start, RunError> {
    let invalid = |field: &str, message: &str| RunError::Invalid {
        field: field.to_string(),
        message: message.to_string(),
    };
    let text = |key: &str| body.get(key).and_then(Value::as_str);
    if let Some(host) = body.get("host")
        && host != LOCAL
    {
        return Err(invalid("host", "must be local"));
    }
    if body.get("worktree").is_some_and(|w| !w.is_null()) {
        return Err(invalid("worktree", "is not supported yet"));
    }
    let size = |key: &str, default: u16| match body.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(value) => value
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .filter(|n| (2..=1000).contains(n))
            .ok_or_else(|| invalid(key, "must be a number from 2 to 1000")),
    };
    let args = match body.get("args") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|a| a.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| invalid("args", "must be a list of strings"))?,
        Some(_) => return Err(invalid("args", "must be a list of strings")),
    };
    Ok(Start {
        agent: text("agent")
            .ok_or_else(|| invalid("agent", "is required"))?
            .to_string(),
        folder: text("folder")
            .ok_or_else(|| invalid("folder", "is required"))?
            .to_string(),
        task: text("task").map(str::to_string),
        args,
        cols: size("cols", 120)?,
        rows: size("rows", 40)?,
        created_by: "app".to_string(),
    })
}

fn started(run: &RunRow) -> Value {
    json!({
        "run": run.to_json(),
        "session": run.session.as_ref().map(|(agent, id)| json!({"agent": agent, "id": id})),
    })
}

/// The runs, latest first: `state` is `live` or `ended`, `session` is
/// `<agent>/<id>` and names the runs that have been in it, and `host`
/// names a host.
async fn list(
    State(state): State<Arc<Pinrail>>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let live = match query.get("state").map(String::as_str) {
        None | Some("") => None,
        Some("live") => Some(true),
        Some("ended") => Some(false),
        Some(_) => return Err(Error::invalid("state", "must be live or ended").into()),
    };
    let session = match query.get("session").filter(|s| !s.is_empty()) {
        None => None,
        Some(session) => Some(
            session
                .split_once('/')
                .map(|(agent, id)| (agent.to_string(), id.to_string()))
                .ok_or_else(|| Error::invalid("session", "must be <agent>/<id>"))?,
        ),
    };
    let filters = RunFilters {
        live,
        session,
        host: query.get("host").filter(|h| !h.is_empty()).cloned(),
    };
    let runs = state.runtime().runs(&filters)?;
    Ok(Json(
        json!({"runs": runs.iter().map(RunRow::to_json).collect::<Vec<_>>()}),
    ))
}

/// A run, with every session it has been in, in order.
async fn show(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let (run, sessions) = state
        .runtime()
        .run(&id)?
        .ok_or_else(|| Error::NotFound(format!("run {id}")))?;
    let mut value = run.to_json();
    value["sessions"] = sessions.iter().map(|s| s.to_json()).collect();
    Ok(Json(value))
}

/// Stops a run's agent, which has 2 seconds to exit, or none with
/// `force`. Answers 202: the run ends when the agent has exited.
async fn stop(
    State(state): State<Arc<Pinrail>>,
    _person: Person,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    let body = match super::parse_body(&body) {
        Ok(body) => body,
        Err(error) => return ApiError(error).into_response(),
    };
    let force = body.get("force").and_then(Value::as_bool).unwrap_or(false);
    match state.runtime().stop(&id, force).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(error) => run_error(error),
    }
}

/// A run error in the shape every API error has.
fn run_error(error: RunError) -> Response {
    let error = match error {
        RunError::Core(error) => return ApiError(error).into_response(),
        error => error,
    };
    let status = match &error {
        RunError::Invalid { field, message } => {
            let mut body = Error::invalid(field.as_str(), message.as_str()).to_json();
            body["message"] = json!(error.to_string());
            return (StatusCode::BAD_REQUEST, Json(body)).into_response();
        }
        RunError::AgentNotFound(_) | RunError::NotFound(_) => StatusCode::NOT_FOUND,
        RunError::Limit(_) => StatusCode::CONFLICT,
        RunError::FolderMissing(_) => StatusCode::UNPROCESSABLE_ENTITY,
        RunError::HostOffline => StatusCode::SERVICE_UNAVAILABLE,
        RunError::StartFailed(_) => StatusCode::BAD_GATEWAY,
        RunError::Core(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    refuse(status, error.code(), error.to_string())
}
