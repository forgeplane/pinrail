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
use crate::db::{Claim, Edited, ItemChange, ItemFilters, Place, State as ItemState, WorkItem};
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
        .route(
            "/api/v1/items/{item}/queue",
            axum::routing::put(queue).delete(unqueue),
        )
        .route(
            "/api/v1/reviews/{id}/items",
            axum::routing::put(set_review_items),
        )
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
        "parents_closed": edited.parents_closed.iter().map(|r| r.to_json()).collect::<Vec<_>>(),
        "cancelled_with": edited.cancelled_with.iter().map(|(r, _)| r.to_json()).collect::<Vec<_>>(),
    }))
}

/// The items the query keeps: `status`, any of open, in_progress, done,
/// cancelled, ready and blocked, comma-separated, by default those open or
/// in progress; `repo`; `project`; `parent`, an item; `created_by`; `q`,
/// words in the title or the body; and `fields=summary`, which leaves the
/// bodies out. Answers `items`.
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
    filters.queued = params.get("queued").is_some_and(|v| v == "true");
    filters.project = params.get("project").cloned();
    filters.created_by = params.get("created_by").cloned();
    filters.text = params.get("q").cloned();
    if let (Some(agent), Some(session)) = (params.get("agent"), params.get("session")) {
        filters.session = Some((agent.clone(), session.clone()));
    }
    if let Some(parent) = params.get("parent") {
        let parent = state
            .items()
            .get(&item_ref(parent)?, filters.repo.as_deref())?;
        filters.parent = Some(parent.id);
    }
    // `fields=summary`: without the bodies, for a list that shows none
    filters.summary = params.get("fields").is_some_and(|v| v == "summary");
    let items = state.items().list(&filters)?;
    let json = |item: &WorkItem| {
        let mut json = item.to_json();
        if filters.summary {
            json.as_object_mut().map(|o| o.remove("body"));
        }
        json
    };
    Ok(Json(
        json!({ "items": items.iter().map(json).collect::<Vec<_>>() }),
    ))
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
/// open sub-item, `parents_closed` or `parent_left_open`.
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
        // the service sets it from the settings
        close_parents: false,
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

/// Puts every round of a review under the work items `items` names, by
/// their keys, or under none with an empty list. Answers 204.
async fn set_review_items(
    State(state): State<Arc<Pinrail>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let body = parse_body(&body)?;
    let actor = actor(&headers, &body);
    let Some(Value::Array(named)) = body.get("items") else {
        return Err(Error::invalid("/items", "list the items by their keys").into());
    };
    let items = named
        .iter()
        .map(|n| match n.as_str() {
            Some(n) => item_ref(n),
            None => Err(Error::invalid("/items", "list the items by their keys")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    state.items().set_review_items(&id, &items, &actor)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Puts an item in the person's queue, or moves it: at the end, or
/// `before` or `after` a queued item, by key. Answers the queue in order,
/// as `items`. Only the person changes the queue.
async fn queue(
    State(state): State<Arc<Pinrail>>,
    headers: HeaderMap,
    Path(item): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = if body.is_empty() {
        json!({})
    } else {
        parse_body(&body)?
    };
    let actor = actor(&headers, &body);
    let place = match (text(&body, "before")?, text(&body, "after")?) {
        (Some(before), None) => Place::Before(item_ref(&before)?),
        (None, Some(after)) => Place::After(item_ref(&after)?),
        (None, None) => Place::End,
        (Some(_), Some(_)) => {
            return Err(Error::invalid("/before", "give before or after, not both").into());
        }
    };
    let queued = state.items().queue(&item_ref(&item)?, &place, &actor)?;
    Ok(Json(json!({
        "items": queued.iter().map(WorkItem::to_json).collect::<Vec<_>>()
    })))
}

/// Takes an item out of the person's queue. Answers the queue in order.
async fn unqueue(
    State(state): State<Arc<Pinrail>>,
    headers: HeaderMap,
    Path(item): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = if body.is_empty() {
        Value::Null
    } else {
        parse_body(&body)?
    };
    let actor = actor(&headers, &body);
    let queued = state.items().unqueue(&item_ref(&item)?, &actor)?;
    Ok(Json(json!({
        "items": queued.iter().map(WorkItem::to_json).collect::<Vec<_>>()
    })))
}
