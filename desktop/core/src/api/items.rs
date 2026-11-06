//! `/api/v1/items`: work items. An item is named in the path by its key,
//! such as `PIN-42`, by its id, or by its number with `?repo=`. A request
//! from the app's window is the person's; any other is the agent's that
//! `by` names, with its `session`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use super::ApiState;
use super::error::ApiError;
use super::parse_body;
use crate::Pinrail;
use crate::db::{Claim, Edited, ItemChange, ItemFilters, State as ItemState};
use crate::error::Error;
use crate::items::{Actor, Breakdown, BreakdownItem, ItemRef, NewItem, Status};
use crate::reviews::Via;

pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/items", get(list).post(add))
        .route("/api/v1/items/next", post(next))
        .route("/api/v1/items/{item}", get(show).patch(edit).delete(delete))
        .route("/api/v1/items/{item}/done", post(done))
        .route("/api/v1/items/{item}/comments", post(comment))
        .route(
            "/api/v1/items/{item}/comments/{comment}",
            patch(edit_comment).delete(delete_comment),
        )
        .route("/api/v1/items/{item}/files/{name}", get(file))
}

/// Who makes the request: the person from the app's window, or the agent
/// named by `by` in the body, with its `session`.
fn actor(headers: &HeaderMap, body: &Value) -> Actor {
    let source = super::reviews::source(headers, body);
    match source.via {
        Via::App => Actor::Person,
        _ => Actor::Agent {
            name: source.by.unwrap_or_else(|| "agent".to_string()),
            session: source.session,
        },
    }
}

fn item_ref(path: &str) -> Result<ItemRef, Error> {
    ItemRef::parse(path).map_err(|e| Error::invalid("/item", e))
}

fn text(body: &Value, key: &str) -> Result<Option<String>, Error> {
    match body.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(Error::invalid(format!("/{key}"), "is not a string")),
    }
}

fn texts(body: &Value, key: &str) -> Result<Vec<String>, Error> {
    match body.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(i, v)| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| Error::invalid(format!("/{key}/{i}"), "is not a string"))
            })
            .collect(),
        Some(_) => Err(Error::invalid(format!("/{key}"), "is not a list")),
    }
}

fn refs(body: &Value, key: &str) -> Result<Vec<ItemRef>, Error> {
    texts(body, key)?
        .iter()
        .enumerate()
        .map(|(i, t)| ItemRef::parse(t).map_err(|e| Error::invalid(format!("/{key}/{i}"), e)))
        .collect()
}

fn edited_json(state: &Pinrail, edited: &Edited) -> Result<Value, Error> {
    Ok(json!({
        "item": state.items().detail(&edited.item)?,
        "now_ready": edited.now_ready.iter().map(|r| r.to_json()).collect::<Vec<_>>(),
        "parent_left_open": edited.parent_left_open.as_ref().map(|r| r.to_json()),
        "cancelled_with": edited.cancelled_with.iter().map(|r| r.to_json()).collect::<Vec<_>>(),
    }))
}

/// The items the query keeps: `status`, any of open, in_progress, done,
/// cancelled, ready and blocked, comma-separated, by default those open or
/// in progress; `repo`; `project`; `parent`, an item; `created_by`; and
/// `q`, words in the title or the body. Answers `items`.
async fn list(
    State(state): State<Arc<Pinrail>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let mut filters = ItemFilters::default();
    if let Some(statuses) = params.get("status") {
        for s in statuses.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            filters.states.push(ItemState::parse(s).ok_or_else(|| {
                Error::invalid(
                    "status",
                    "must be open, in_progress, done, cancelled, ready or blocked",
                )
            })?);
        }
    }
    filters.repo = params.get("repo").cloned();
    filters.project = params.get("project").cloned();
    filters.created_by = params.get("created_by").cloned();
    filters.text = params.get("q").cloned();
    if let Some(parent) = params.get("parent") {
        let parent = state
            .items()
            .get(&item_ref(parent)?, filters.repo.as_deref())?;
        filters.parent = Some(parent.id);
    }
    let items = state.items().list(&filters)?;
    Ok(Json(json!({
        "items": items.iter().map(|i| i.to_json()).collect::<Vec<_>>(),
    })))
}

/// Adds an item from `repo`, `title` and, optionally, `body`, `parent`,
/// `depends_on`, `link` and the `attachments` its body shows; or, with
/// `items`, a breakdown of several, each with a `ref` the others name it
/// by, all added or none. Answers 201 with the item, or with `items`, each
/// with its `ref`.
async fn add(
    State(state): State<Arc<Pinrail>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let body = parse_body(&body)?;
    let actor = actor(&headers, &body);
    let repo = text(&body, "repo")?.unwrap_or_default();
    let parent = text(&body, "parent")?
        .map(|p| ItemRef::parse(&p).map_err(|e| Error::invalid("/parent", e)))
        .transpose()?;
    if let Some(items) = body.get("items") {
        let Value::Array(items) = items else {
            return Err(Error::invalid("/items", "is not a list").into());
        };
        let items = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let at = |k: &str| format!("/items/{i}/{k}");
                let field = |k: &str| -> Result<Option<String>, Error> {
                    match item.get(k) {
                        None | Some(Value::Null) => Ok(None),
                        Some(Value::String(s)) => Ok(Some(s.clone())),
                        Some(_) => Err(Error::invalid(at(k), "is not a string")),
                    }
                };
                Ok(BreakdownItem {
                    r#ref: field("ref")?.unwrap_or_default(),
                    title: field("title")?.unwrap_or_default(),
                    body: field("body")?.unwrap_or_default(),
                    parent: field("parent")?,
                    depends_on: texts(item, "depends_on").map_err(|_| {
                        Error::invalid(at("depends_on"), "is not a list of strings")
                    })?,
                    link: field("link")?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let added = state.items().add_breakdown(
            &Breakdown {
                repo,
                parent,
                items,
            },
            &actor,
        )?;
        return Ok((
            StatusCode::CREATED,
            Json(json!({
                "items": added
                    .iter()
                    .map(|(r, item)| json!({"ref": r, "item": item.to_json()}))
                    .collect::<Vec<_>>(),
            })),
        ));
    }
    let new = NewItem {
        repo,
        title: text(&body, "title")?.unwrap_or_default(),
        body: text(&body, "body")?.unwrap_or_default(),
        parent,
        depends_on: refs(&body, "depends_on")?,
        link: text(&body, "link")?,
    };
    let item = state.items().add(&new, body.get("attachments"), &actor)?;
    Ok((StatusCode::CREATED, Json(state.items().detail(&item)?)))
}

/// The next ready item of `repo`, claimed with `claim: true`: `item`, or
/// null when nothing is ready.
async fn next(
    State(state): State<Arc<Pinrail>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = parse_body(&body)?;
    let actor = actor(&headers, &body);
    let repo = text(&body, "repo")?.unwrap_or_default();
    let claim = body.get("claim").and_then(Value::as_bool).unwrap_or(false);
    let item = state.items().next(&repo, claim, &actor)?;
    Ok(Json(json!({
        "item": item.map(|i| state.items().detail(&i)).transpose()?,
    })))
}

/// An item with its relations, sub-items, files, reviews, sessions and
/// timeline.
async fn show(
    State(state): State<Arc<Pinrail>>,
    Path(item): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let found = state
        .items()
        .get(&item_ref(&item)?, params.get("repo").map(String::as_str))?;
    Ok(Json(state.items().detail(&found)?))
}

/// Changes an item: any of `title`, `body`, `link` (null removes it),
/// `status`, `claim` (`claim`, `take` or `release`), `parent` (null takes
/// it out), `depends_on` and `no_depends_on`, `repo` to move it, the
/// `attachments` to add to its body and the files to `detach`, and a
/// `comment` that goes with the change. Answers the item, the items it made
/// ready, `now_ready`, and its parent when it closed the parent's last
/// open sub-item, `parent_left_open`.
async fn edit(
    State(state): State<Arc<Pinrail>>,
    headers: HeaderMap,
    Path(item): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = parse_body(&body)?;
    let actor = actor(&headers, &body);
    let status = text(&body, "status")?
        .map(|s| {
            Status::parse(&s).ok_or_else(|| {
                Error::invalid("/status", "must be open, in_progress, done or cancelled")
            })
        })
        .transpose()?;
    let claim = match text(&body, "claim")?.as_deref() {
        None => None,
        Some("claim") => Some(Claim::Take { take: false }),
        Some("take") => Some(Claim::Take { take: true }),
        Some("release") => Some(Claim::Release),
        Some(_) => return Err(Error::invalid("/claim", "must be claim, take or release").into()),
    };
    let parent = match body.get("parent") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(p)) => Some(Some(
            ItemRef::parse(p).map_err(|e| Error::invalid("/parent", e))?,
        )),
        Some(_) => return Err(Error::invalid("/parent", "is not an item").into()),
    };
    let link = match body.get("link") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(l)) if l.trim().is_empty() => Some(None),
        Some(Value::String(l)) => Some(Some(l.clone())),
        Some(_) => return Err(Error::invalid("/link", "is not a string").into()),
    };
    let change = ItemChange {
        title: text(&body, "title")?,
        body: text(&body, "body")?,
        link,
        status,
        claim,
        parent,
        depends_on: refs(&body, "depends_on")?,
        no_depends_on: refs(&body, "no_depends_on")?,
        repo: text(&body, "repo")?,
        files: state.items().check_files(body.get("attachments"))?,
        detach: texts(&body, "detach")?,
        comment: text(&body, "comment")?,
    };
    let edited = state.items().edit(
        &item_ref(&item)?,
        params.get("repo").map(String::as_str),
        &change,
        &actor,
    )?;
    Ok(Json(edited_json(&state, &edited)?))
}

/// Marks an item done, with a `comment` that says what was done. An agent
/// can only when nothing of it waits on the person and its sub-items are
/// closed: 409 says what is still open.
async fn done(
    State(state): State<Arc<Pinrail>>,
    headers: HeaderMap,
    Path(item): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = parse_body(&body)?;
    let actor = actor(&headers, &body);
    let comment = text(&body, "comment")?;
    let edited = state.items().done(
        &item_ref(&item)?,
        params.get("repo").map(String::as_str),
        comment.as_deref(),
        &actor,
    )?;
    Ok(Json(edited_json(&state, &edited)?))
}

/// Deletes an item and its sub-items: the person alone can. Answers how
/// many went, `deleted`.
async fn delete(
    State(state): State<Arc<Pinrail>>,
    headers: HeaderMap,
    Path(item): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let actor = actor(&headers, &Value::Null);
    let deleted = state.items().delete(
        &item_ref(&item)?,
        params.get("repo").map(String::as_str),
        &actor,
    )?;
    Ok(Json(json!({ "deleted": deleted })))
}

/// Adds a comment, `body`, with the `attachments` it shows. Answers 201
/// with the comment.
async fn comment(
    State(state): State<Arc<Pinrail>>,
    headers: HeaderMap,
    Path(item): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let body = parse_body(&body)?;
    let actor = actor(&headers, &body);
    let comment = state.items().comment(
        &item_ref(&item)?,
        params.get("repo").map(String::as_str),
        &text(&body, "body")?.unwrap_or_default(),
        body.get("attachments"),
        &actor,
    )?;
    Ok((StatusCode::CREATED, Json(comment.to_json())))
}

/// Changes a comment's `body`; its author alone can.
async fn edit_comment(
    State(state): State<Arc<Pinrail>>,
    headers: HeaderMap,
    Path((item, comment)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = parse_body(&body)?;
    let actor = actor(&headers, &body);
    let edited = state.items().edit_comment(
        &item_ref(&item)?,
        params.get("repo").map(String::as_str),
        &comment,
        &text(&body, "body")?.unwrap_or_default(),
        &actor,
    )?;
    Ok(Json(edited.to_json()))
}

/// Deletes a comment: its author can, and the person can delete any. An
/// agent names itself with `?by=` and `?session=`.
async fn delete_comment(
    State(state): State<Arc<Pinrail>>,
    headers: HeaderMap,
    Path((item, comment)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<StatusCode, ApiError> {
    let who = json!({"by": params.get("by"), "session": params.get("session")});
    let actor = actor(&headers, &who);
    state.items().delete_comment(
        &item_ref(&item)?,
        params.get("repo").map(String::as_str),
        &comment,
        &actor,
    )?;
    Ok(StatusCode::NO_CONTENT)
}

/// One of an item's files, by name, as a download.
async fn file(
    State(state): State<Arc<Pinrail>>,
    Path((item, name)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let found = state.items().file(
        &item_ref(&item)?,
        params.get("repo").map(String::as_str),
        &name,
    )?;
    super::reviews::serve_attachment(&state, &found, &name).await
}
