//! `/api/v1/reviews`: submit, read, list, rounds, long-poll wait, decide,
//! withdraw, discard, events, and the files a review carries.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde_json::{Value, json};

use super::ApiState;
use super::error::ApiError;
use super::parse_body;
use crate::Pinrail;
use crate::db::{MessageRow, OpenStage, SessionFilters, SessionRow, StageRow, StatusKind};
use crate::error::Error;
use crate::markdown::Head;
use crate::reviews::{Filters, Kind, Source, Status, Via, iso};

const DEFAULT_WAIT: u64 = 300;
const MAX_WAIT: u64 = 600;

pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/reviews", post(submit).get(list))
        .route("/api/v1/reviews/validate", post(validate))
        .route("/api/v1/reviews/{id}", get(show).delete(delete_review))
        .route("/api/v1/reviews/{id}/rounds", get(rounds))
        .route("/api/v1/reviews/{id}/view", post(view))
        .route("/api/v1/reviews/{id}/wait", get(wait))
        .route("/api/v1/reviews/{id}/decision", post(decide))
        .route("/api/v1/reviews/{id}/withdraw", post(withdraw))
        .route("/api/v1/reviews/{id}/discard", post(discard))
        .route("/api/v1/reviews/{id}/viewed", post(viewed))
        .route("/api/v1/reviews/{id}/star", put(star).delete(unstar))
        .route("/api/v1/reviews/{id}/events", get(events))
        .route("/api/v1/reviews/{id}/attachments/{name}", get(attachment))
        .route("/api/v1/sessions", get(sessions))
        .route("/api/v1/stages", get(stages))
        .route("/api/v1/sessions/{agent}/{id}", get(session))
        .route("/api/v1/sessions/{agent}/{id}/status", post(status))
        .route("/api/v1/sessions/{agent}/{id}/messages", post(send_message))
        .route(
            "/api/v1/sessions/{agent}/{id}/messages/{message}",
            delete(delete_message),
        )
        .route(
            "/api/v1/sessions/{agent}/{id}/statuses/{status}/attachments/{name}",
            get(note_attachment),
        )
        .route(
            "/api/v1/sessions/{agent}/{id}/archive",
            put(archive).delete(unarchive),
        )
}

async fn submit(State(state): State<Arc<Pinrail>>, body: Bytes) -> Result<Response, ApiError> {
    let body = parse_body(&body)?;
    // the same submission as a review still pending answers that review
    let (review, created) = state.reviews().submit_once(&body, None)?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(review.to_json(true))).into_response())
}

/// The checks a submission gets, with nothing stored: 200 naming the plugin
/// version that would render it, none for questions, or the 422 `submit`
/// would answer.
async fn validate(State(state): State<Arc<Pinrail>>, body: Bytes) -> Result<Json<Value>, ApiError> {
    let body = parse_body(&body)?;
    let plugin = state.reviews().validate(&body)?;
    Ok(Json(json!({
        "valid": true,
        "kind": if plugin.is_some() { Kind::Review } else { Kind::Question }.as_str(),
        "plugin": plugin.as_ref().map(|p| p.name.clone()),
        "plugin_version": plugin.as_ref().map(|p| p.version.clone()),
    })))
}

/// A page of reviews, newest first, without payloads, in an envelope:
/// `reviews`, `total`, `has_more` and `next_cursor`, and `facets` with
/// `include=facets`. Walk everything with `cursor=<next_cursor>`; number
/// pages with `offset`.
async fn list(
    State(state): State<Arc<Pinrail>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let mut filters = filters(&params)?;
    // the reviews of a work item, by its key, or its number in `repo`
    if let Some(item) = params.get("item").filter(|i| !i.is_empty()) {
        let item = crate::items::ItemRef::parse(item).map_err(|e| Error::invalid("/item", e))?;
        let found = state
            .items()
            .get(&item, params.get("repo").map(String::as_str))?;
        filters.item = Some(found.id);
    }
    let mut facets = false;
    if let Some(include) = params.get("include") {
        for name in include.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            match name {
                "facets" => facets = true,
                other => {
                    return Err(
                        Error::invalid("/include", format!("unknown include {other}")).into(),
                    );
                }
            }
        }
    }
    let listing = state.reviews().listing(&filters, facets)?;
    let mut body = json!({
        "reviews": listing.reviews.iter().map(|r| r.to_json(false)).collect::<Vec<_>>(),
        "total": listing.total,
        "has_more": listing.has_more,
        "next_cursor": listing.next_cursor,
    });
    if let Some(f) = listing.facets {
        body["facets"] =
            json!({ "plugins": f.plugins, "repos": f.repos, "unassigned": f.unassigned });
    }
    Ok(Json(body))
}

/// Whether the caller wants the review as markdown, and opening how:
/// `?format=markdown`, or an Accept header that names text/markdown ahead
/// of JSON; `&head=command` for the lines a command prints rather than a
/// document's heading.
fn wants_markdown(headers: &HeaderMap, params: &HashMap<String, String>) -> Option<Head> {
    let head = match params.get("head").map(String::as_str) {
        Some("command") => Head::Command,
        _ => Head::Document,
    };
    if let Some(f) = params.get("format") {
        return (f == "markdown" || f == "md").then_some(head);
    }
    let wanted = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| {
            let md = accept.find("text/markdown");
            let json = accept.find("application/json");
            matches!((md, json), (Some(m), Some(j)) if m < j)
                || matches!((md, json), (Some(_), None))
        });
    wanted.then_some(head)
}

/// A review as the caller asked for it: markdown with its round placed
/// in the chain, or the JSON everything else reads. The markdown may run the
/// plugin's template, so it is rendered where slow work belongs, off the
/// threads that answer requests.
async fn review_response(
    state: &Pinrail,
    review: &crate::reviews::Review,
    markdown: Option<Head>,
) -> Result<Response, ApiError> {
    let Some(head) = markdown else {
        return Ok(Json(review.to_json(true)).into_response());
    };
    let rounds = state.reviews().rounds(&review.id)?;
    let round = (rounds.len() > 1)
        .then(|| {
            rounds
                .iter()
                .position(|r| r.id == review.id)
                .map(|i| (i + 1, rounds.len()))
        })
        .flatten();
    let template = match review.kind {
        Kind::Question => Some(crate::reviews::questions::TEMPLATE.to_string()),
        Kind::Review => state
            .plugins()
            .fetch_review(&review.plugin, review.plugin_bundle.as_deref())
            .ok()
            .and_then(|p| p.decision_template.clone()),
    };
    let json = review.to_json(true);
    let text = tokio::task::spawn_blocking(move || {
        crate::markdown::render_in(&json, round, template.as_deref(), None, head)
    })
    .await
    .map_err(|e| Error::Internal(format!("rendering the review: {e}")))?;
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        text,
    )
        .into_response())
}

async fn show(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let review = state.reviews().get(&id)?;
    review_response(&state, &review, wants_markdown(&headers, &params)).await
}

/// Opens a review for the app's frame: a pending review first moves to the
/// version of its plugin installed now, when that version takes it. The
/// answer is the plugin it renders with, the bundle and the address of its
/// view, which never changes and is cached, the bundle the installation
/// holds now, and `refused` when the installed version does not take the
/// review, saying why.
async fn view(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let opened = state.reviews().open(&id)?;
    let Some(bundle) = opened.review.plugin_bundle.clone() else {
        return Err(Error::NotFound(format!("the bundle of review {id}")).into());
    };
    Ok(Json(json!({
        "url": format!("/bundles/{bundle}/view/index.html"),
        "bundle": bundle,
        "plugin": opened.plugin.to_json(),
        "review": opened.review.to_json(true),
        "refused": opened.refused,
        "installed": opened.installed,
    })))
}

async fn rounds(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let rounds = state.reviews().rounds(&id)?;
    Ok(Json(Value::Array(
        rounds.iter().map(|r| r.to_json(false)).collect(),
    )))
}

async fn wait(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let timeout = params
        .get("timeout")
        .and_then(|t| t.parse::<u64>().ok())
        .unwrap_or(DEFAULT_WAIT)
        .min(MAX_WAIT);
    match state
        .reviews()
        .wait(&id, Duration::from_secs(timeout))
        .await?
    {
        Some(review) => review_response(&state, &review, wants_markdown(&headers, &params)).await,
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}

async fn decide(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let body = parse_body(&body)?;
    let Some(data) = body.get("data") else {
        return Err(Error::invalid("/data", "is required").into());
    };
    // `dry_run=true`: the checks a decision gets, and nothing decided
    if params.get("dry_run").is_some_and(|v| v == "true") {
        state.reviews().check_decision(&id, data)?;
        return Ok(Json(json!({ "valid": true, "data": data })).into_response());
    }
    let note = body.get("agent_note").and_then(Value::as_str);
    // the bundle the person's view showed, when the app sends it
    let shown = body.get("bundle").and_then(Value::as_str);
    let review = state
        .reviews()
        .decide(&id, data, note, shown, &source(&headers, &body))?;
    review_response(&state, &review, wants_markdown(&headers, &params)).await
}

async fn withdraw(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = parse_body(&body)?;
    let reason = body.get("reason").and_then(Value::as_str);
    Ok(Json(
        state
            .reviews()
            .withdraw(&id, reason, &source(&headers, &body))?
            .to_json(true),
    ))
}

async fn discard(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = if body.is_empty() {
        json!({})
    } else {
        parse_body(&body)?
    };
    let reason = body.get("reason").and_then(Value::as_str);
    Ok(Json(
        state
            .reviews()
            .discard(&id, reason, &source(&headers, &body))?
            .to_json(true),
    ))
}

/// The header the `pinrail` CLI sends with every request: `cli`.
pub(crate) const CLIENT_HEADER: &str = "x-pinrail-client";

/// Where a decision, a discard or a withdrawal comes from. The app's requests carry its
/// own origin, which a browser sets and a web page cannot; the CLI names
/// itself in a header, and sends the agent's session it runs in; anything
/// else is another client of the API. A program on this machine can claim
/// any of these, so this records what a client says, not proof.
/// Deletes a review with every round of it. A round still waiting is
/// discarded first, so the agent waiting on it is told to stop. Answers
/// the `ids` that went.
async fn delete_review(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let ids = state
        .reviews()
        .delete_review(&id, &source(&headers, &Value::Null))?;
    Ok(Json(json!({ "ids": ids })))
}

pub(super) fn source(headers: &HeaderMap, body: &Value) -> Source {
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    if origin.is_some_and(|o| super::shell_origins().iter().any(|s| s == o)) {
        return Source {
            via: Via::App,
            session: None,
            by: None,
        };
    }
    let cli = headers
        .get(CLIENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == "cli");
    let session = body
        .get("session")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let by = body
        .get("by")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .map(str::to_string);
    Source {
        via: if cli { Via::Cli } else { Via::Api },
        session,
        by,
    }
}

/// Stars a round of a review: 204, and 422 for a question.
async fn star(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.reviews().star(&id, true)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Removes the star of a round: 204.
async fn unstar(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.reviews().star(&id, false)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn viewed(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state.reviews().mark_viewed(&id)?;
    Ok(Json(json!({ "ok": true })))
}

async fn events(
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let events = state.reviews().events(&id)?;
    Ok(Json(Value::Array(
        events.iter().map(|e| e.to_json()).collect(),
    )))
}

fn filters(params: &HashMap<String, String>) -> Result<Filters, ApiError> {
    let mut statuses = Vec::new();
    if let Some(s) = params.get("status") {
        let mut unknown = Vec::new();
        for name in s.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            match Status::parse(name) {
                Some(status) => statuses.push(status),
                None => unknown.push(name.to_string()),
            }
        }
        if !unknown.is_empty() {
            return Err(Error::invalid(
                "/status",
                format!("unknown status {}", unknown.join(", ")),
            )
            .into());
        }
    }
    let text = |key: &str| params.get(key).filter(|v| !v.is_empty()).cloned();
    let kind = match text("kind") {
        None => None,
        Some(name) => Some(
            Kind::parse(&name)
                .ok_or_else(|| Error::invalid("/kind", format!("unknown kind {name}")))?,
        ),
    };
    Ok(Filters {
        statuses,
        plugin: text("plugin"),
        kind,
        agent: text("agent"),
        session: text("session"),
        repo: text("repo"),
        project: text("project"),
        workflow: text("workflow"),
        reference: text("ref"),
        run_id: text("run_id"),
        item: None,
        text: text("q"),
        include_revised: params.get("include_revised").is_some_and(|v| v == "true"),
        starred: params.get("starred").is_some_and(|v| v == "true"),
        cursor: text("cursor"),
        oldest_first: false,
        limit: params
            .get("limit")
            .and_then(|l| l.parse().ok())
            .unwrap_or(0),
        offset: params
            .get("offset")
            .and_then(|o| o.parse().ok())
            .unwrap_or(0),
    })
}

/// The bytes of a file the review carries, as a download and nothing else:
/// whatever an agent uploaded (an `.html`, an `.svg` with a script), a
/// browser neither sniffs it nor renders it in this origin, where it could
/// call the API.
async fn attachment(
    State(state): State<Arc<Pinrail>>,
    Path((id, name)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let review = state.reviews().get(&id)?;
    let Some(carried) = review.attachments.iter().find(|a| a.name == name) else {
        return Ok(super::guard::refuse(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("review {id} carries no attachment \"{name}\""),
        ));
    };
    serve_attachment(&state, carried, &name).await
}

/// The bytes of a file a note carries, as a review's are served.
async fn note_attachment(
    State(state): State<Arc<Pinrail>>,
    Path((agent, id, status, name)): Path<(String, String, i64, String)>,
) -> Result<Response, ApiError> {
    let carried = state
        .reviews()
        .statuses(&agent, &id)?
        .into_iter()
        .find(|s| s.id == status)
        .and_then(|s| s.attachments.into_iter().find(|a| a.name == name));
    let Some(carried) = carried else {
        return Ok(super::guard::refuse(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("status {status} of session {id} carries no attachment \"{name}\""),
        ));
    };
    serve_attachment(&state, &carried, &name).await
}

/// A stored file as a download under `name`: never sniffed, never rendered
/// in this origin.
pub(super) async fn serve_attachment(
    state: &Pinrail,
    carried: &crate::attachments::ReviewAttachment,
    name: &str,
) -> Result<Response, ApiError> {
    let path = state.attachments().path(&carried.sha256);
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| Error::Internal(format!("attachment {}: {e}", carried.sha256)))?;
    let encoded: String = name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (header::CONTENT_LENGTH, carried.size.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename*=UTF-8''{encoded}"),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
            (
                header::CONTENT_SECURITY_POLICY,
                "sandbox; default-src 'none'".to_string(),
            ),
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable".to_string(),
            ),
        ],
        axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(file)),
    )
        .into_response())
}

/// A session as the API gives it: who, where, when, and what it holds.
fn session_json(session: &SessionRow) -> Value {
    json!({
        "agent": session.agent,
        "id": session.id,
        "repo": session.repo,
        "ref": session.reference,
        "folder": session.folder,
        "status": session.status.as_ref().map(|s| s.to_json()),
        "title": session.title,
        "archived_at": session.archived_at.map(crate::reviews::iso),
        "reported_at": session.reported_at.map(crate::reviews::iso),
        "project": session.project.as_ref().map(crate::db::ProjectRef::to_json),
        "first_at": iso(session.first_at),
        "last_at": iso(session.last_at),
        "total": { "questions": session.questions, "reviews": session.reviews },
        "waiting": { "questions": session.waiting_questions, "reviews": session.waiting_reviews },
    })
}

/// A page of sessions, those waiting for the person first, then the
/// newest: `sessions`, `total`, and `facets` with the projects of all the
/// sessions. Filters: `agent`, `repo` (`-` for no project), `q` and
/// `waiting=true`; pages by `limit` and `offset`.
async fn sessions(
    State(state): State<Arc<Pinrail>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let text = |key: &str| params.get(key).filter(|v| !v.is_empty()).cloned();
    let number = |key: &str| params.get(key).and_then(|v| v.parse().ok()).unwrap_or(0);
    let filters = SessionFilters {
        agent: text("agent"),
        repo: text("repo"),
        project: text("project"),
        text: text("q"),
        waiting: params.get("waiting").is_some_and(|v| v == "true"),
        include_archived: params.get("include_archived").is_some_and(|v| v == "true"),
        limit: number("limit"),
        offset: number("offset"),
    };
    let (sessions, total) = state.reviews().sessions(&filters)?;
    let (repos, unassigned) = state.reviews().session_projects()?;
    Ok(Json(json!({
        "sessions": sessions.iter().map(session_json).collect::<Vec<_>>(),
        "total": total,
        "facets": {"repos": repos, "unassigned": unassigned},
    })))
}

/// Archives a session: 204, and 404 for an unknown one.
async fn archive(
    State(state): State<Arc<Pinrail>>,
    Path((agent, id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state.reviews().archive_session(&agent, &id, true)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Brings an archived session back: 204.
async fn unarchive(
    State(state): State<Arc<Pinrail>>,
    Path((agent, id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state.reviews().archive_session(&agent, &id, false)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The stages agents started in any session, newest first: `stages`, each
/// with its session. `q` keeps those whose names hold every word, and
/// `limit` (at most 50, 20 by default) caps them.
async fn stages(
    State(state): State<Arc<Pinrail>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let text = params.get("q").filter(|v| !v.is_empty());
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20usize)
        .clamp(1, 50);
    let stages = state.reviews().stages(text.map(String::as_str), limit)?;
    Ok(Json(json!({
        "stages": stages.iter().map(StageRow::to_json).collect::<Vec<_>>(),
    })))
}

/// One session as a thread: the session, its questions and reviews oldest
/// first, the questions with their payloads, its statuses, the comments its
/// agent wrote on work items and what it did to them, and the person's
/// messages to the agent.
async fn session(
    State(state): State<Arc<Pinrail>>,
    Path((agent, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let (session, items) = state.reviews().session(&agent, &id)?;
    let statuses = state.reviews().statuses(&agent, &id)?;
    let messages = state.reviews().messages(&agent, &id)?;
    // what the session's agent said about work items, which the thread
    // shows among its statuses
    let comments = state
        .items()
        .session_comments(&agent, &id)?
        .into_iter()
        .map(|(comment, item)| {
            let mut json = comment.to_json();
            json["item"] = item.to_json();
            json
        })
        .collect::<Vec<_>>();
    // what it did to work items, which the thread shows as short lines
    let item_events = state
        .items()
        .session_item_events(&agent, &id)?
        .into_iter()
        .map(|(event, item)| {
            let mut json = event.to_json();
            json["item"] = item.to_json();
            json
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "session": session_json(&session),
        "statuses": statuses.iter().map(|s| s.to_json()).collect::<Vec<_>>(),
        "comments": comments,
        "item_events": item_events,
        "messages": messages.iter().map(MessageRow::to_json).collect::<Vec<_>>(),
        "items": items
            .iter()
            .map(|r| r.to_json(r.kind == Kind::Question))
            .collect::<Vec<_>>(),
    })))
}

/// What the agent says it is doing in the session, with `pinrail status`:
/// `text`, one line, and its `kind`: `step`, the default, `start` for the
/// start of a task, which names it, `done` once it has finished, or `note`
/// for a Markdown message, whose text may span lines; with the `origin` it
/// works in. Answers `messages`, the person's messages it handed to the
/// agent, oldest first, and `stage`, the stage now open with its `text` and
/// `at`, or `null`.
async fn status(
    State(state): State<Arc<Pinrail>>,
    Path((agent, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let text = body.get("text").and_then(Value::as_str).unwrap_or("");
    let kind = match body.get("kind").and_then(Value::as_str) {
        None => StatusKind::Step,
        Some(kind) => StatusKind::parse(kind)
            .ok_or_else(|| Error::invalid("kind", "must be step, start, done or note"))?,
    };
    let (messages, stage) = state.reviews().status(
        &agent,
        &id,
        text,
        kind,
        body.get("origin"),
        body.get("attachments"),
    )?;
    Ok(Json(json!({
        "messages": messages.iter().map(MessageRow::to_json).collect::<Vec<_>>(),
        "stage": stage.as_ref().map(OpenStage::to_json),
    })))
}

/// Sends the agent of the session a message, `text`, which waits until
/// the agent next reports a status. Answers 201 with the message.
async fn send_message(
    State(state): State<Arc<Pinrail>>,
    Path((agent, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let text = body.get("text").and_then(Value::as_str).unwrap_or("");
    let message = state.reviews().send_message(&agent, &id, text)?;
    Ok((StatusCode::CREATED, Json(message.to_json())))
}

/// Deletes a message the agent has not received yet. Answers 204, or 409
/// once the agent has received it.
async fn delete_message(
    State(state): State<Arc<Pinrail>>,
    Path((agent, id, message)): Path<(String, String, i64)>,
) -> Result<StatusCode, ApiError> {
    state.reviews().delete_message(&agent, &id, message)?;
    Ok(StatusCode::NO_CONTENT)
}
