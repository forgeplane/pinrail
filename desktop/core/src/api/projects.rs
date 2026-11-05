//! `/api/v1/projects`: the person's projects, named sets of repositories
//! the inbox and the sidebar can group by, and the repositories in none.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, put};
use axum::{Json, Router};
use serde_json::Value;

use super::ApiState;
use super::error::ApiError;
use super::parse_body;
use crate::Pinrail;
use crate::error::Error;

pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/projects", get(list).post(create))
        .route("/api/v1/projects/{id}", patch(rename).delete(delete))
        .route("/api/v1/repos/{repo}/project", put(set_repo))
}

/// Every project with its repositories, by name, and the repositories in
/// no project: `projects` and `loose`. Each repository and project carries
/// its `sessions`, what is `waiting` in it and its latest activity,
/// `last_at`.
async fn list(State(state): State<Arc<Pinrail>>) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.projects().listing()?))
}

/// Creates a project from `name` and, optionally, `repos`, moving any
/// repository that is in another project: 201 with the project.
async fn create(
    State(state): State<Arc<Pinrail>>,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let body = parse_body(&body)?;
    let name = text(&body, "name")?;
    let repos = match body.get("repos") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(repos)) => repos
            .iter()
            .enumerate()
            .map(|(i, r)| {
                r.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| Error::invalid(format!("/repos/{i}"), "is not a string"))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(Error::invalid("/repos", "is not a list").into()),
    };
    let project = state.projects().create(&name, &repos)?;
    Ok((StatusCode::CREATED, Json(project)))
}

/// Renames a project with `name`: the project, or 404.
async fn rename(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = parse_body(&body)?;
    let name = text(&body, "name")?;
    Ok(Json(state.projects().rename(&id, &name)?))
}

/// Deletes a project, leaving its repositories in none: 204, or 404.
async fn delete(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.projects().delete(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Puts the repository, URL-encoded in the path, in the project
/// `project_id`, taking it out of the one it was in; `null` puts it in
/// none: 204, or 404 for an unknown project.
async fn set_repo(
    State(state): State<Arc<Pinrail>>,
    Path(repo): Path<String>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let body = parse_body(&body)?;
    let project = match body.get("project_id") {
        None => return Err(Error::invalid("/project_id", "is required").into()),
        Some(Value::Null) => None,
        Some(Value::String(id)) => Some(id.as_str()),
        Some(_) => return Err(Error::invalid("/project_id", "is not a string or null").into()),
    };
    state.projects().set_repo(&repo, project)?;
    Ok(StatusCode::NO_CONTENT)
}

/// A required string field of the body.
fn text(body: &Value, key: &str) -> Result<String, Error> {
    match body.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        None | Some(Value::Null) => Err(Error::invalid(format!("/{key}"), "is required")),
        Some(_) => Err(Error::invalid(format!("/{key}"), "is not a string")),
    }
}
