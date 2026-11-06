//! `/api/v1/projects`: the person's projects, named sets of repositories
//! the inbox and the sidebar can group by, and the repositories in none.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
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
        .route("/api/v1/projects/{id}", patch(update).delete(delete))
        .route(
            "/api/v1/projects/{id}/image",
            put(set_image).get(image).delete(remove_image),
        )
        .route("/api/v1/repos/{repo}/project", put(set_repo))
        .route(
            "/api/v1/repos/{repo}",
            axum::routing::delete(delete_repo).patch(set_prefix),
        )
}

/// Every project with its repositories, by name, and the repositories in
/// no project: `projects` and `loose`. Each repository and project carries
/// its `sessions`, what is `waiting` in it and its latest activity,
/// `last_at`.
async fn list(State(state): State<Arc<Pinrail>>) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.projects().listing()?))
}

/// Creates a project from `name` and, optionally, a `description` and
/// `repos`, moving any repository that is in another project: 201 with the
/// project.
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
    let details = crate::db::Details {
        description: optional_text(&body, "description")?,
        color: optional_text(&body, "color")?,
        emoji: optional_text(&body, "emoji")?,
    };
    let project = state.projects().create(&name, &details, &repos)?;
    Ok((StatusCode::CREATED, Json(project)))
}

/// Changes a project's `name`, `description`, `color` or `emoji`, any of
/// them; an empty or `null` description, colour or emoji removes it: the
/// project, or 404.
async fn update(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = parse_body(&body)?;
    // given as a string, removed as null, left as it is when missing
    let removable = |key: &str| -> Result<Option<Option<String>>, Error> {
        match body.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(_) => Ok(Some(optional_text(&body, key)?)),
        }
    };
    let change = crate::db::Change {
        name: match body.get("name") {
            None => None,
            Some(_) => Some(text(&body, "name")?),
        },
        description: removable("description")?,
        color: removable("color")?,
        emoji: removable("emoji")?,
    };
    if change == crate::db::Change::default() {
        return Err(Error::invalid("", "give a name, a description, a color or an emoji").into());
    }
    Ok(Json(state.projects().update(&id, &change)?))
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

/// An optional string field of the body: `None` when it is missing or `null`.
fn optional_text(body: &Value, key: &str) -> Result<Option<String>, Error> {
    match body.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(Error::invalid(format!("/{key}"), "is not a string")),
    }
}

/// Sets the PNG the project's avatar shows, sent as the body: the project,
/// 404, or 422 for a body that is not a PNG or is over 512 KB.
async fn set_image(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.projects().set_image(&id, &body)?))
}

/// Removes the project's PNG: the project, or 404.
async fn remove_image(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.projects().remove_image(&id)?))
}

/// The project's PNG, as an image and nothing else; 404 when it has none.
/// The app asks for it with the image's hash in the address, so a new
/// image is a new address and the old one can be kept.
async fn image(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let Some(path) = state.projects().image_path(&id)? else {
        return Ok(super::guard::refuse(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("project {id} has no image"),
        ));
    };
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| Error::Internal(format!("project {id}'s image: {e}")))?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (
                header::CONTENT_SECURITY_POLICY,
                "sandbox; default-src 'none'",
            ),
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable",
            ),
        ],
        bytes,
    )
        .into_response())
}

/// Deletes a repository from the app, URL-encoded in the path: what waits
/// from it is discarded, and the agents waiting are told to stop; then
/// every round of its reviews and its sessions are deleted, and it leaves
/// its project. Answers how many `reviews` and `sessions` went, and how
/// many were `discarded`.
async fn delete_repo(
    State(state): State<Arc<Pinrail>>,
    Path(repo): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let source = super::reviews::source(&headers, &Value::Null);
    Ok(Json(state.reviews().delete_repo(&repo, &source)?))
}

/// A required string field of the body.
fn text(body: &Value, key: &str) -> Result<String, Error> {
    match body.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        None | Some(Value::Null) => Err(Error::invalid(format!("/{key}"), "is required")),
        Some(_) => Err(Error::invalid(format!("/{key}"), "is not a string")),
    }
}

/// Sets the repository's work item key `prefix`, two to five capital
/// letters that no other repository has or had: its items' old keys still
/// find them. Answers 204.
async fn set_prefix(
    State(state): State<Arc<Pinrail>>,
    Path(repo): Path<String>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let body = parse_body(&body)?;
    let prefix = text(&body, "prefix")?;
    state.items().set_prefix(&repo, &prefix)?;
    Ok(StatusCode::NO_CONTENT)
}
