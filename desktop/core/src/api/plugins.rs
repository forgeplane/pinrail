//! `/api/v1/plugins`: registered plugins, the versions reviews render with, reload, and
//! adding a plugin directory.

use std::path::Path as FsPath;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use super::parse_body;
use crate::Wicket;
use crate::error::Error;

pub fn routes() -> Router<Arc<Wicket>> {
    Router::new()
        .route("/api/v1/plugins", get(index))
        .route("/api/v1/plugins/reload", post(reload))
        .route("/api/v1/plugins/dirs", post(add_dir))
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
) -> Result<Json<Value>, Error> {
    state.plugins().versions(&name).map(Json)
}

/// The source and the options an install or an inspect takes:
/// `{source, link?, force?, ref?, path?}`.
fn install_request(body: &Bytes) -> Result<(String, crate::install::Options), Error> {
    let body = parse_body(body)?;
    let Some(source) = body.get("source").and_then(Value::as_str) else {
        return Err(Error::invalid("/source", "is required"));
    };
    let options = crate::install::Options {
        link: body.get("link").and_then(Value::as_bool).unwrap_or(false),
        force: body.get("force").and_then(Value::as_bool).unwrap_or(false),
        reference: body.get("ref").and_then(Value::as_str).map(str::to_string),
        path: body.get("path").and_then(Value::as_str).map(str::to_string),
    };
    Ok((source.to_string(), options))
}

/// What installing a source would do, for the dialog to show before the
/// person says yes: the manifest's plugin, the origin, the build command,
/// what is installed under that name. Fetches the source and drops it.
async fn inspect(State(state): State<Arc<Wicket>>, body: Bytes) -> Result<Json<Value>, Error> {
    let (source, options) = install_request(&body)?;
    let worker = state.clone();
    tokio::task::spawn_blocking(move || {
        crate::install::inspect(&worker.db, &worker.registry, &source, options, &|_| {})
    })
    .await
    .map_err(|e| Error::Internal(e.to_string()))?
    .map(Json)
}

/// Starts installing one plugin: `{source, link?, force?, ref?, path?}`.
/// Answers at once with the job to follow; a build can take a minute.
async fn install(State(state): State<Arc<Wicket>>, body: Bytes) -> Result<Response, Error> {
    let (source, options) = install_request(&body)?;
    let id = state.jobs.start(&source);
    let job_id = id.clone();
    let worker = state.clone();
    tokio::task::spawn_blocking(move || {
        let jobs = worker.jobs.clone();
        let progress = |p| jobs.note(&job_id, p);
        let outcome =
            crate::install::install(&worker.db, &worker.registry, &source, options, &progress)
                .and_then(|record| {
                    worker.plugins().announce()?;
                    worker
                        .registry
                        .get(&record.name)
                        .map(|p| p.to_json())
                        .ok_or_else(|| {
                            Error::Internal(format!(
                                "{} was installed and is not registered",
                                record.name
                            ))
                        })
                });
        worker.jobs.finish(&job_id, outcome);
    });
    Ok((StatusCode::ACCEPTED, Json(json!({ "job": id }))).into_response())
}

/// What is new for an installed plugin, asked of its source.
async fn updates(
    State(state): State<Arc<Wicket>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, Error> {
    let record = state
        .db
        .installed_plugins()?
        .into_iter()
        .find(|r| r.name == name)
        .ok_or(Error::NotFound(name))?;
    let registry = state.registry.clone();
    let answer =
        tokio::task::spawn_blocking(move || crate::install::check_updates(&registry, &record))
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(Json(answer))
}

/// Installs a plugin again from where it came. Asks the source first: a
/// link or a pinned source is refused with that said, nothing new answers
/// `{state: "up_to_date"}` at once, and otherwise the install runs as a
/// job to follow, into the same line or a new one beside it.
async fn update(
    State(state): State<Arc<Wicket>>,
    Path(name): Path<String>,
) -> Result<Response, Error> {
    let record = state
        .db
        .installed_plugins()?
        .into_iter()
        .find(|r| r.name == name)
        .ok_or_else(|| Error::NotFound(name.clone()))?;
    let registry = state.registry.clone();
    let checked = record.clone();
    let answer =
        tokio::task::spawn_blocking(move || crate::install::check_updates(&registry, &checked))
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
    match answer["state"].as_str().unwrap_or("unknown") {
        "linked" => {
            return Err(Error::invalid(
                "/name",
                format!("{name} is a link: it is always what its folder holds"),
            ));
        }
        "pinned" => {
            let at = answer["tag"]
                .as_str()
                .or(answer["ref"].as_str())
                .unwrap_or("this version");
            return Err(Error::invalid(
                "/name",
                format!("{name} is pinned to {at}; install another ref to move it"),
            ));
        }
        "up_to_date" => {
            return Ok(
                Json(json!({ "state": "up_to_date", "version": record.version })).into_response(),
            );
        }
        _ => {}
    }
    let (source, options) = crate::install::source_of(&record);
    let id = state.jobs.start(&source);
    let job_id = id.clone();
    let worker = state.clone();
    tokio::task::spawn_blocking(move || {
        let jobs = worker.jobs.clone();
        let progress = |p| jobs.note(&job_id, p);
        let outcome =
            crate::install::install(&worker.db, &worker.registry, &source, options, &progress)
                .and_then(|record| {
                    worker.plugins().announce()?;
                    worker
                        .registry
                        .get(&record.name)
                        .map(|p| p.to_json())
                        .ok_or_else(|| {
                            Error::Internal(format!(
                                "{} was installed and is not registered",
                                record.name
                            ))
                        })
                });
        worker.jobs.finish(&job_id, outcome);
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "job": id, "state": "updating" })),
    )
        .into_response())
}

/// Removes an installed plugin: the record and the store entries no
/// review renders from; the ones a review still uses stay, and the answer
/// names them. A built-in has no record and cannot be removed.
async fn remove(
    State(state): State<Arc<Wicket>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, Error> {
    state.plugins().remove(&name).map(Json)
}

/// An install job as it stands: its step, its log so far, and how it ended.
async fn job(
    State(state): State<Arc<Wicket>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    state
        .jobs
        .get(&id)
        .map(|job| Json(job.to_json()))
        .ok_or(Error::NotFound(id))
}

async fn reload(State(state): State<Arc<Wicket>>) -> Result<Json<Value>, Error> {
    let count = state.plugins().reload()?;
    Ok(Json(json!({ "ok": true, "count": count })))
}

async fn add_dir(State(state): State<Arc<Wicket>>, body: Bytes) -> Result<Json<Value>, Error> {
    let body = parse_body(&body)?;
    let Some(dir) = body.get("dir").and_then(Value::as_str) else {
        return Err(Error::invalid("/dir", "is required"));
    };
    let count = state.plugins().add_dir(FsPath::new(dir))?;
    Ok(Json(json!({
        "ok": true,
        "count": count,
        "dirs": state.plugins().dirs().iter().map(|d| d.display().to_string()).collect::<Vec<_>>(),
    })))
}
