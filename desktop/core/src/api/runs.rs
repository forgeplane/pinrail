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
use crate::db::{Claim, ItemChange, RunFilters, RunRow};
use crate::error::Error;
use crate::items::{Actor, ItemRef};
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
        .route(
            "/api/v1/runtime/restarted",
            get(restarted).delete(dismiss_restart),
        )
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
/// 201 with the run and its session, by Pinrail's id, whose
/// `agent_session` is null until an agent that names its session itself
/// reports it. A work `item`, by its key, is claimed for the new session,
/// which takes the claim over from another; an unknown item answers 404
/// and starts nothing.
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
    request.agent_args = state.settings().agent_args(&request.agent);
    // the CLI names itself, with the person token of /runtime/cli_control
    if headers
        .get(super::reviews::CLIENT_HEADER)
        .is_some_and(|v| v == "cli")
    {
        request.created_by = "cli".to_string();
    }
    let item = match body.get("item") {
        None | Some(Value::Null) => None,
        Some(Value::String(key)) => match ItemRef::parse(key) {
            Ok(item) => Some(item),
            Err(e) => return ApiError::from(Error::invalid("/item", e)).into_response(),
        },
        Some(_) => {
            return ApiError::from(Error::invalid("/item", "is not an item's key")).into_response();
        }
    };
    // an item that is not there starts nothing
    if let Some(item) = &item {
        match state.items().get(item, None) {
            Ok(found) => request.item = Some(found.key.to_string()),
            Err(error) => return ApiError::from(error).into_response(),
        }
    }
    let local_id = body.get("local_id").and_then(Value::as_str);
    match state.runtime().start_once(local_id, &request).await {
        Ok(run) => {
            type_first_task(&state, &request, &run);
            let mut answer = started(&run);
            if let (Some(item), Some((agent, session))) = (&item, &run.session)
                && let Err(refused) = claim_for(&state, item, agent, session)
            {
                answer["claim_refused"] = Value::from(refused);
            }
            (StatusCode::CREATED, Json(answer)).into_response()
        }
        Err(error) => run_error(error),
    }
}

/// Sends the first task as the person's first message to the run's
/// session, for an agent that takes no task in its command. It is typed
/// once the agent is idle. A start repeated with its `local_id` sends it
/// once.
fn type_first_task(state: &Arc<Pinrail>, request: &crate::runtime::Start, run: &RunRow) {
    let Some(agent) = pinrail_runtime::adapter(&request.agent) else {
        return;
    };
    let task = agent
        .start(&pinrail_runtime::StartRequest {
            task: request.task.as_deref(),
            extra_args: &request.args,
            session_id: "",
        })
        .and_then(|plan| plan.type_task);
    let (Some(task), Some((agent, session))) = (task, &run.session) else {
        return;
    };
    let local_id = format!("first-task-{}", run.id);
    match state
        .reviews()
        .send_message(agent, session, &task, Some(&local_id), "type")
    {
        Ok(_) => super::reviews::deliver_later(state, agent, session),
        Err(error) => tracing::warn!("sending the first task failed: {error:?}"),
    }
}

/// Claims `item` for a session a start made, taking the claim over from
/// another. The run has started by then, so a claim the item's rules
/// refuse, such as on an item on hold, is answered beside the run, as
/// `claim_refused` with the reason.
fn claim_for(state: &Pinrail, item: &ItemRef, agent: &str, session: &str) -> Result<(), String> {
    let change = ItemChange {
        claim: Some(Claim::Take { take: true }),
        ..ItemChange::default()
    };
    let actor = Actor::Agent {
        name: agent.to_string(),
        session: Some(session.to_string()),
    };
    state
        .items()
        .edit(item, None, &change, &actor)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Resumes a stopped session in a new terminal of `cols` by `rows`, in
/// the folder it worked in, or in `folder`, which the person chose when
/// that one is gone and which becomes the session's. Answers 201 with the
/// run; 409
/// `already_running`, with the run when it runs in Pinrail, or the
/// `process` that runs it in another terminal and whether it can be
/// resumed `anyway`, which `anyway: true` then does; 422 `folder_missing`
/// or `not_resumable`.
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
    let agent_args = state.settings().agent_args(&agent);
    let request = Resume {
        agent,
        session: id,
        cols,
        rows,
        created_by: created_by.to_string(),
        anyway: body.get("anyway").and_then(Value::as_bool) == Some(true),
        folder: body
            .get("folder")
            .and_then(Value::as_str)
            .map(str::to_string),
        agent_args,
    };
    match state.runtime().resume(&request).await {
        Ok(run) => (StatusCode::CREATED, Json(started(&run))).into_response(),
        Err(error) => run_error(error),
    }
}

/// The runs a restart of the computer ended, latest first, with the title
/// of each one's session, unless the person resumed or dismissed them or
/// turned `/runtime/after_restart` off.
async fn restarted(State(state): State<Arc<Pinrail>>) -> Response {
    if state.settings().value("/runtime/after_restart") == "off" {
        return Json(json!({"runs": []})).into_response();
    }
    let runs = match state.runtime().restarted() {
        Ok(runs) => runs,
        Err(error) => return run_error(error),
    };
    let runs: Vec<Value> = runs
        .into_iter()
        .map(|(run, title)| {
            let mut value = run.to_json();
            value["title"] = json!(title);
            value
        })
        .collect();
    Json(json!({ "runs": runs })).into_response()
}

/// Hides the runs a restart ended until the next restart. The person's.
async fn dismiss_restart(State(state): State<Arc<Pinrail>>, _person: Person) -> Response {
    match state.runtime().dismiss_restart() {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
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
        item: None,
        args,
        agent_args: Vec::new(),
        cols: size(body, "cols", 120)?,
        rows: size(body, "rows", 40)?,
        created_by: "app".to_string(),
    })
}

fn started(run: &RunRow) -> Value {
    json!({
        "run": run.to_json(),
        "session": run.session_json(),
    })
}

/// The runs, latest first: `state` is `live` or `ended`, `session` is
/// Pinrail's id of a session, or `<agent>/<id>`, and names the runs that
/// have been in it, and `host` names a host.
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
    let session = query
        .get("session")
        .filter(|s| !s.is_empty())
        .map(|session| {
            session
                .split_once('/')
                .map(|(agent, id)| (agent.to_string(), id.to_string()))
                // Pinrail's id names one session whatever its agent
                .unwrap_or_else(|| (String::new(), session.clone()))
        });
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
    let folders = state
        .runtime()
        .recent_folders(state.config().agent_history.as_deref())?;
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
        RunError::AlreadyRunning {
            run,
            process,
            anyway,
        } => {
            let mut body =
                json!({"error": error.code(), "message": error.to_string(), "violations": []});
            body["run"] = run.as_ref().map_or(Value::Null, |r| r.to_json());
            body["process"] = process.map_or(Value::Null, |pid| json!({ "pid": pid }));
            // whether the person can resume it anyway
            body["anyway"] = json!(anyway);
            return (StatusCode::CONFLICT, Json(body)).into_response();
        }
        RunError::Limit(_) | RunError::NotTyped(_) => StatusCode::CONFLICT,
        RunError::FolderMissing(_) | RunError::NotResumable(_) => StatusCode::UNPROCESSABLE_ENTITY,
        RunError::HostOffline => StatusCode::SERVICE_UNAVAILABLE,
        RunError::StartFailed(_) => StatusCode::BAD_GATEWAY,
        RunError::Core(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    refuse(status, error.code(), error.to_string())
}
