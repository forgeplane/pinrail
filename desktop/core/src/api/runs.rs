//! The agents Pinrail starts: start one in a folder, list them, read one
//! with the sessions it has been in, and stop one. Starting and stopping
//! are the person's. A host can be read too, for diagnosis.

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
use crate::runtime::{LOCAL, Resume, RunError, Start};

pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/runs", post(start).get(list))
        .route("/api/v1/agents/startable", get(startable))
        .route("/api/v1/folders/recent", get(recent_folders))
        .route("/api/v1/runs/{run}", get(show))
        .route("/api/v1/runs/{run}/output", get(output))
        .route("/api/v1/runs/{run}/screen", get(screen))
        .route("/api/v1/runs/{run}/stop", post(stop))
        .route("/api/v1/sessions/{agent}/{id}/resume", post(resume))
        .route("/api/v1/hosts/{host}", get(host))
}

/// A host, for diagnosis: the link's state, what the host says of itself,
/// and its terminals. Only the person reads it, as the terminals' labels
/// and processes describe every agent.
async fn host(
    State(state): State<Arc<Pinrail>>,
    _person: Person,
    Path(host): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if host != LOCAL {
        return Err(Error::NotFound(format!("host {host}")).into());
    }
    let mut value = json!({"id": LOCAL, "link": state.runtime().status()});
    if let Some(link) = state.runtime().host() {
        if let Ok(info) = link.call("host.info", json!({})).await {
            value["info"] = info;
        }
        if let Ok(terminals) = link.call("terminal.list", json!({})).await {
            value["terminals"] = terminals;
        }
    }
    Ok(Json(value))
}

/// Starts `agent` in `folder`, with an optional first `task`, extra `args`
/// and the terminal's `cols` and `rows`. `host` is `local`, the only host
/// so far. A `local_id` makes the call idempotent for 10 minutes. Answers
/// 201 with the run and its session, which is null for an agent that
/// names its session only once it starts.
async fn start(
    State(state): State<Arc<Pinrail>>,
    _person: Person,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let mut request = match parse_start(&body) {
        Ok(request) => request,
        Err(error) => return run_error(error),
    };
    // the CLI names itself, with the person token of /runtime/cli_control
    if headers
        .get(super::reviews::CLIENT_HEADER)
        .is_some_and(|v| v == "cli")
    {
        request.created_by = "cli".to_string();
    }
    let local_id = body.get("local_id").and_then(Value::as_str);
    match state.runtime().start_once(local_id, &request).await {
        Ok(run) => (StatusCode::CREATED, Json(started(&run))).into_response(),
        Err(error) => run_error(error),
    }
}

/// Resumes a stopped session in a new terminal of `cols` by `rows`, in
/// the folder it worked in. Answers 201 with the run; 409
/// `already_running`, with the run when it runs in Pinrail; 422
/// `folder_missing` or `not_resumable`.
async fn resume(
    State(state): State<Arc<Pinrail>>,
    _person: Person,
    headers: axum::http::HeaderMap,
    Path((agent, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Response {
    if let Some(host) = body.get("host")
        && !host.is_null()
        && host != LOCAL
    {
        return run_error(RunError::Invalid {
            field: "host".to_string(),
            message: "must be local".to_string(),
        });
    }
    let (cols, rows) = match (size(&body, "cols", 120), size(&body, "rows", 40)) {
        (Ok(cols), Ok(rows)) => (cols, rows),
        (Err(error), _) | (_, Err(error)) => return run_error(error),
    };
    let created_by = if headers
        .get(super::reviews::CLIENT_HEADER)
        .is_some_and(|v| v == "cli")
    {
        "cli"
    } else {
        "app"
    };
    let request = Resume {
        agent,
        session: id,
        cols,
        rows,
        created_by: created_by.to_string(),
    };
    match state.runtime().resume(&request).await {
        Ok(run) => (StatusCode::CREATED, Json(started(&run))).into_response(),
        Err(error) => run_error(error),
    }
}

/// A terminal's size in `body[key]`: 2 to 1000, or `default` when absent.
fn size(body: &Value, key: &str, default: u16) -> Result<u16, RunError> {
    match body.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(value) => value
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .filter(|n| (2..=1000).contains(n))
            .ok_or_else(|| RunError::Invalid {
                field: key.to_string(),
                message: "must be a number from 2 to 1000".to_string(),
            }),
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
        cols: size(body, "cols", 120)?,
        rows: size(body, "rows", 40)?,
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

/// The agents Pinrail can start, with whether each is installed and its
/// version, for the Start an agent dialog.
/// The answer is the last the host gave, which it is asked again in the
/// background when that is due, or on `refresh=true`.
async fn startable(
    State(state): State<Arc<Pinrail>>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let refresh = query.get("refresh").is_some_and(|v| v == "true");
    match state.runtime().startable(refresh).await {
        Ok((body, due)) => {
            if due {
                crate::runtime::check_startable_later(state.clone(), chrono::Utc::now());
            }
            Json(body).into_response()
        }
        Err(error) => run_error(error),
    }
}

/// The folders agents worked in lately, for the Start an agent dialog.
async fn recent_folders(State(state): State<Arc<Pinrail>>) -> Result<Json<Value>, ApiError> {
    let folders = state.runtime().recent_folders()?;
    Ok(Json(json!({ "folders": folders })))
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
    // what the person started it with, to start it again
    let (task, args) = state.runtime().run_launch(&id)?.unwrap_or_default();
    value["task"] = json!(task);
    value["args"] = json!(args);
    Ok(Json(value))
}

/// The last `lines` lines (20 by default, at most 500) a run's terminal
/// showed, such as those of an agent that exited as it started. Only the
/// person reads a terminal.
async fn output(
    State(state): State<Arc<Pinrail>>,
    _person: Person,
    Path(id): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let lines = query
        .get("lines")
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(20)
        .min(500);
    match state.runtime().output(&id, lines).await {
        Ok(text) => Json(json!({ "text": text })).into_response(),
        Err(error) => run_error(error),
    }
}

/// A run's screen as a paint any emulator reads, with its columns and
/// rows: for an ended run, the screen it ended with. Only the person reads
/// a terminal.
async fn screen(
    State(state): State<Arc<Pinrail>>,
    _person: Person,
    Path(id): Path<String>,
) -> Response {
    match state.runtime().screen(&id).await {
        Ok(screen) => Json(screen).into_response(),
        Err(error) => run_error(error),
    }
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
        RunError::AlreadyRunning(run) => {
            let mut body =
                json!({"error": error.code(), "message": error.to_string(), "violations": []});
            body["run"] = run.as_ref().map_or(Value::Null, |r| r.to_json());
            return (StatusCode::CONFLICT, Json(body)).into_response();
        }
        RunError::Limit(_) => StatusCode::CONFLICT,
        RunError::FolderMissing(_) | RunError::NotResumable(_) => StatusCode::UNPROCESSABLE_ENTITY,
        RunError::HostOffline => StatusCode::SERVICE_UNAVAILABLE,
        RunError::StartFailed(_) => StatusCode::BAD_GATEWAY,
        RunError::Core(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    refuse(status, error.code(), error.to_string())
}
