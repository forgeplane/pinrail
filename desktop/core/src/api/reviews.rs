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
use crate::db::{
    MessageRow, OpenStage, Report, ReportEvent, SessionFilters, SessionRow, StageRow, StatusKind,
};
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
        .route("/api/v1/elements", get(elements))
        .route(
            "/api/v1/sessions/{agent}/{id}",
            get(session).patch(rename).delete(delete_session),
        )
        .route("/api/v1/sessions/{agent}/{id}/status", post(status))
        .route("/api/v1/sessions/{agent}/{id}/reports", post(report))
        .route(
            "/api/v1/sessions/{agent}/{id}/seen",
            put(seen).delete(unseen),
        )
        .route("/api/v1/sessions/{agent}/{id}/messages", post(send_message))
        .route(
            "/api/v1/sessions/{agent}/{id}/messages/ack",
            post(acknowledge_messages),
        )
        .route(
            "/api/v1/sessions/{agent}/{id}/messages/{message}",
            delete(delete_message),
        )
        .route(
            "/api/v1/sessions/{agent}/{id}/messages/{message}/type",
            post(type_message),
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
    // an agent names its session by its own id, and the app by Pinrail's
    if let (Some(agent), Some(session)) = (&filters.agent, &filters.session)
        && let Some(found) = state.reviews().find_session(agent, session)?
    {
        filters.session = Some(found);
    }
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
    let review = state.reviews().decide(
        &id,
        data,
        note,
        shown,
        &resolved_source(&state, &headers, &body)?,
    )?;
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
            .withdraw(&id, reason, &resolved_source(&state, &headers, &body)?)?
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
            .discard(&id, reason, &resolved_source(&state, &headers, &body)?)?
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

/// As [`source`], with the session, which the client names by the agent's
/// own id, by Pinrail's id: the session of the agent `by` names, created
/// if it is new.
pub(super) fn resolved_source(
    state: &Pinrail,
    headers: &HeaderMap,
    body: &Value,
) -> Result<Source, Error> {
    let mut source = source(headers, body);
    source.session = state
        .reviews()
        .session_of(source.by.as_deref(), source.session.as_deref())?;
    Ok(source)
}

/// Stars a round of a review: 204, and 422 for a question.
async fn star(
    _person: super::guard::Person,
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.reviews().star(&id, true)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Removes the star of a round: 204.
async fn unstar(
    _person: super::guard::Person,
    State(state): State<Arc<Pinrail>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.reviews().star(&id, false)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn viewed(
    _person: super::guard::Person,
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

/// A turn of the session, as the thread shows it.
fn turn_json(turn: &crate::db::TurnRow) -> Value {
    json!({
        "turn": turn.turn,
        "agent_turn": turn.agent_turn,
        "started_at": iso(turn.started_at),
        "ended_at": turn.ended_at.map(iso),
        "end_kind": turn.end_kind,
        "prompt": turn.prompt,
        "prompt_cut": turn.prompt_cut,
        // the latest reply, where the turn ended, and every one in order
        "reply": turn.replies.last().map(|r| &r.text),
        "reply_cut": turn.replies.last().is_some_and(|r| r.cut),
        "replies": turn.replies.iter().map(|r| json!({
            "at": crate::reviews::iso(r.at),
            "text": r.text,
            "cut": r.cut,
        })).collect::<Vec<_>>(),
    })
}

/// Whether Pinrail can resume the session, which takes the agent's own id
/// for it, and whether also while it still runs in another terminal, as
/// the agent's adapter says.
fn resume_abilities(session: &SessionRow) -> Value {
    let adapter = pinrail_runtime::adapter(&session.agent);
    json!({
        "resume": session.agent_session.is_some()
            && adapter.is_some_and(|a| a.resume("session").is_some()),
        "while_running": adapter.is_some_and(|a| a.resumes_while_running()),
    })
}

/// A session as the API gives it: who, where, when, and what it holds.
pub(crate) fn session_json(session: &SessionRow) -> Value {
    json!({
        "agent": session.agent,
        "id": session.id,
        "agent_session": session.agent_session,
        "repo": session.repo,
        "ref": session.reference,
        "folder": session.folder,
        "status": session.status.as_ref().map(|s| s.to_json()),
        "title": session.title,
        "name": session.name,
        "generated_name": session.generated_name,
        "element": session.element,
        "archived_at": session.archived_at.map(crate::reviews::iso),
        "project": session.project.as_ref().map(crate::db::ProjectRef::to_json),
        "first_at": iso(session.first_at),
        "last_at": iso(session.last_at),
        "total": { "questions": session.questions, "reviews": session.reviews },
        "waiting": { "questions": session.waiting_questions, "reviews": session.waiting_reviews },
        "attention": session.attention,
        "resume": resume_abilities(session),
        // whether Pinrail can type a message into its agent's terminal
        "types": pinrail_runtime::adapter(&session.agent).is_some_and(|a| a.submit().is_some()),
        "presence": session.presence.as_ref().map(|p| {
            let attention = crate::presence::Attention::parse(&session.attention)
                .unwrap_or(crate::presence::Attention::Unknown);
            crate::presence::to_json(p, attention)
        }),
    })
}

/// A page of sessions, those waiting for the person first, then the
/// newest, or with `sort=priority` by the attention they ask of the person:
/// `sessions`, `total`, and `facets` with the projects of all the sessions.
/// Filters: `agent`, `repo` (`-` for no project), `q`, `waiting=true` and
/// `attention`, a comma-separated list such as `needs_you,finished`; pages
/// by `limit` and `offset`.
async fn sessions(
    State(state): State<Arc<Pinrail>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let text = |key: &str| params.get(key).filter(|v| !v.is_empty()).cloned();
    let number = |key: &str| params.get(key).and_then(|v| v.parse().ok()).unwrap_or(0);
    let filters = SessionFilters {
        element: match text("element") {
            None => None,
            Some(n) => Some(
                n.parse()
                    .map_err(|_| Error::invalid("element", "must be an atomic number"))?,
            ),
        },
        agent: text("agent"),
        repo: text("repo"),
        project: text("project"),
        text: text("q"),
        waiting: params.get("waiting").is_some_and(|v| v == "true"),
        attention: match text("attention") {
            None => Vec::new(),
            Some(list) => list
                .split(',')
                .map(|a| {
                    crate::presence::Attention::parse(a.trim())
                        .map(|a| a.as_str().to_string())
                        .ok_or_else(|| {
                            Error::invalid(
                                "attention",
                                "must list needs_you, finished, working, idle, stopped or unknown",
                            )
                        })
                })
                .collect::<Result<_, _>>()?,
        },
        priority: match text("sort").as_deref() {
            None | Some("recent") => false,
            Some("priority") => true,
            Some(_) => return Err(Error::invalid("sort", "must be recent or priority").into()),
        },
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

/// The person saw the session: 204, and 404 for an unknown session.
async fn seen(
    _person: super::guard::Person,
    State(state): State<Arc<Pinrail>>,
    Path((agent, id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state.reviews().set_seen(&agent, &id, true)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Mark as unread: the session is Finished again if its last turn
/// finished. 204, and 404 for an unknown session.
async fn unseen(
    _person: super::guard::Person,
    State(state): State<Arc<Pinrail>>,
    Path((agent, id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state.reviews().set_seen(&agent, &id, false)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Archives a session, and stops its agent if Pinrail runs it: 204, and
/// 404 for an unknown one.
async fn archive(
    _person: super::guard::Person,
    State(state): State<Arc<Pinrail>>,
    Path((agent, id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state.reviews().archive_session(&agent, &id, true)?;
    #[cfg(unix)]
    if let Err(error) = state.runtime().stop_session(&agent, &id).await {
        eprintln!("pinrail: stopping the run of session {id} failed: {error}");
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The chemical elements sessions are named after: `elements`, each with
/// its number, symbol, name, category, rarity, place on the periodic table,
/// one sentence about it, and `sessions`, how many sessions it names,
/// archived ones included.
async fn elements(State(state): State<Arc<Pinrail>>) -> Result<Json<Value>, ApiError> {
    let counts = state.reviews().element_counts()?;
    let elements: Vec<Value> = crate::elements::all()
        .iter()
        .map(|e| {
            let mut json = serde_json::to_value(e).unwrap_or(Value::Null);
            json["sessions"] = json!(counts.get(&e.number).copied().unwrap_or(0));
            json
        })
        .collect();
    Ok(Json(json!({ "elements": elements })))
}

/// Renames a session, for the person: `name`, or `null` to clear the
/// person's name, so that the session goes by its generated name. 204.
async fn rename(
    _person: super::guard::Person,
    State(state): State<Arc<Pinrail>>,
    Path((agent, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<StatusCode, ApiError> {
    let name = match body.get("name") {
        Some(Value::String(name)) => Some(name.as_str()),
        Some(Value::Null) => None,
        _ => return Err(Error::invalid("/name", "must be a string or null").into()),
    };
    state.reviews().rename_session(&agent, &id, name)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Deletes a session, for the person: stops its run if Pinrail runs it,
/// withdraws its reviews that still wait, gives up the work items it holds,
/// and deletes it with its statuses, messages, presence and turns. Its
/// decided reviews stay in the history. 204; 404 when there is none.
async fn delete_session(
    _person: super::guard::Person,
    State(state): State<Arc<Pinrail>>,
    Path((agent, id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    #[cfg(unix)]
    if let Err(error) = state.runtime().stop_session(&agent, &id).await {
        eprintln!("pinrail: stopping the run of session {id} failed: {error}");
    }
    state.reviews().delete_session(&agent, &id)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Brings an archived session back: 204.
async fn unarchive(
    _person: super::guard::Person,
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
/// messages to the agent. A link from before Pinrail's ids names the
/// session by the agent's own id, which finds it when no session has that
/// id; the answer's `session.id` then differs from the path's.
async fn session(
    State(state): State<Arc<Pinrail>>,
    Path((agent, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let (session, items) = match state.reviews().session(&agent, &id) {
        Err(Error::NotFound(message)) => match state.reviews().find_session(&agent, &id)? {
            Some(found) => state.reviews().session(&agent, &found)?,
            None => return Err(Error::NotFound(message).into()),
        },
        found => found?,
    };
    let id = session.id.clone();
    let statuses = state.reviews().statuses(&agent, &id)?;
    let messages = state.reviews().messages(&agent, &id)?;
    let turns = state.reviews().turns(&agent, &id)?;
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
        "turns": turns.iter().map(turn_json).collect::<Vec<_>>(),
        "items": items
            .iter()
            .map(|r| r.to_json(r.kind == Kind::Question))
            .collect::<Vec<_>>(),
    })))
}

/// What the agent says it is doing in the session, which the path names by
/// the agent's own id, with `pinrail status`:
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
    let (id, messages, stage) = state.reviews().status(
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
        "name": state.reviews().session_name(&agent, &id)?,
    })))
}

/// What the session's hooks report, from `pinrail hook`, naming the
/// session by the agent's own id: the `event`
/// (`started`, `prompt`, `working`, `needs_input`, `turn_ended` or
/// `ended`), `at`, when the hook fired, and what the event carries:
/// `source` with `started`, `wait` (`permission` or `question`) with
/// `needs_input`, `end` (`done`, `failed` or `interrupted`) with
/// `turn_ended`, and in `detail` the `tool`, `summary`, `prompt`, `reply`,
/// `error`, `reason`, the agent's own `turn_id`, and `prompt_cut` or
/// `reply_cut` when the CLI cut the text. `transcript_path` names the
/// agent's own transcript of the session. `process` names the agent's `pid` and when it
/// `started`, and `origin` where the session works. Answers 204, also for a
/// report that arrived after a newer one.
///
/// With `Authorization: Run <token>` from a run's own agent process, a
/// report for another session than the run's is refused with 403
/// `run_mismatch`, except a session start, which moves the run to that
/// session.
async fn report(
    State(state): State<Arc<Pinrail>>,
    Path((agent, id)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    let report = parse_report(&body)?;
    let keep_texts = state
        .settings()
        .value("/runtime/record_turns")
        .as_bool()
        .unwrap_or(true);
    let now = chrono::Utc::now();
    if agent.trim().is_empty() || id.trim().is_empty() {
        return Err(Error::invalid("session", "name the agent and the session").into());
    }
    // the path names the session by the agent's own id
    let admitted = match state
        .runtime()
        .admit_report(run_token(&headers), &agent, &id, &report)?
    {
        Ok(admitted) => admitted,
        Err(mismatch) => {
            return Ok(super::guard::refuse(
                StatusCode::FORBIDDEN,
                "run_mismatch",
                mismatch.0,
            ));
        }
    };
    // the agent left the session it was in: Pinrail ends it, as the host
    // does when an agent exits, which a session whose state comes from its
    // terminal takes too
    if let Some((agent, cleared)) = &admitted.cleared {
        let ended = Report {
            event: ReportEvent::Ended {
                reason: Some("clear".to_string()),
            },
            transcript_path: None,
            agent_turn: None,
            cut: false,
            origin: Default::default(),
            source: crate::db::Source::Host,
            // Pinrail's own conclusion, as the run moves: the report that
            // moved it may have fired before the session's last record
            at: now,
            ..report.clone()
        };
        state.reviews().report(agent, cleared, &ended, false, now)?;
    }
    let id = admitted.session;
    state
        .reviews()
        .report(&agent, &id, &report, keep_texts, now)?;
    // a prompt confirms a message typed into the terminal, and an idle
    // agent can take the next one
    #[cfg(unix)]
    if let ReportEvent::Prompt {
        prompt: Some(prompt),
    } = &report.event
    {
        let _ = state.runtime().confirm(&agent, &id, prompt, report.cut);
    }
    deliver_later(&state, &agent, &id);
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// The run token of `Authorization: Run <token>`.
fn run_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Run ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

fn parse_report(body: &Value) -> Result<Report, Error> {
    let text = |key: &str| body.get(key).and_then(Value::as_str);
    let detail = |key: &str| {
        body.get("detail")
            .and_then(|d| d.get(key))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let one_of = |key: &str, values: &[&str]| -> Result<String, Error> {
        text(key)
            .filter(|v| values.contains(v))
            .map(str::to_string)
            .ok_or_else(|| Error::invalid(key, format!("must be {}", values.join(", "))))
    };
    let event = match text("event") {
        Some("started") => ReportEvent::Started {
            source: match text("source") {
                None => None,
                Some(_) => Some(one_of(
                    "source",
                    &["startup", "resume", "clear", "compact", "fork"],
                )?),
            },
        },
        Some("prompt") => ReportEvent::Prompt {
            prompt: detail("prompt"),
        },
        Some("working") => ReportEvent::Working,
        Some("needs_input") => ReportEvent::NeedsInput {
            kind: one_of("wait", &["permission", "question"])?,
            tool: detail("tool").map(|s| s.chars().take(200).collect()),
            summary: detail("summary").map(|s| s.chars().take(200).collect()),
        },
        Some("turn_ended") => ReportEvent::TurnEnded {
            end: one_of("end", &["done", "failed", "interrupted"])?,
            // a failed turn's error stands for its reply
            reply: detail("reply").or_else(|| detail("error")),
        },
        Some("ended") => ReportEvent::Ended {
            reason: detail("reason"),
        },
        _ => {
            return Err(Error::invalid(
                "event",
                "must be started, prompt, working, needs_input, turn_ended or ended",
            ));
        }
    };
    let at = match text("at") {
        Some(at) => crate::reviews::parse_datetime(at)
            .ok_or_else(|| Error::invalid("at", "must be an ISO 8601 time"))?,
        None => chrono::Utc::now(),
    };
    let process = body.get("process").and_then(|p| {
        Some((
            p.get("pid")?.as_i64()?,
            p.get("started")?.as_str()?.to_string(),
        ))
    });
    let origin = body
        .get("origin")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let flag = |key: &str| {
        body.get("detail")
            .and_then(|d| d.get(key))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    Ok(Report {
        cut: flag("prompt_cut") || flag("reply_cut"),
        agent_turn: detail("turn_id"),
        transcript_path: text("transcript_path")
            .filter(|p| !p.is_empty())
            .map(str::to_string),
        event,
        at,
        process,
        origin,
        source: crate::db::Source::Hook,
    })
}

/// Sends the agent of the session a message, `text`: typed into its
/// terminal once it is idle while Pinrail runs it, and otherwise waiting
/// until the agent next reports a status. Answers 201 with the message and
/// its `route`.
async fn send_message(
    _person: super::guard::Person,
    State(state): State<Arc<Pinrail>>,
    Path((agent, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let text = body.get("text").and_then(Value::as_str).unwrap_or("");
    let local_id = body.get("local_id").and_then(Value::as_str);
    #[cfg(unix)]
    let route = state.runtime().message_route(&agent, &id);
    #[cfg(not(unix))]
    let route = "queue";
    let message = state
        .reviews()
        .send_message(&agent, &id, text, local_id, route)?;
    deliver_later(&state, &agent, &id);
    Ok((StatusCode::CREATED, Json(message.to_json())))
}

/// Types the messages waiting for the session, in the background.
pub(super) fn deliver_later(state: &Arc<Pinrail>, agent: &str, id: &str) {
    #[cfg(unix)]
    {
        let (app, agent, id) = (state.clone(), agent.to_string(), id.to_string());
        tokio::spawn(async move {
            let _ = app.runtime().deliver(&agent, &id).await;
        });
    }
    #[cfg(not(unix))]
    let _ = (state, agent, id);
}

/// Types a message that shows as unconfirmed into the agent's terminal
/// again, as the person asks. Answers 202; the message's state follows as
/// events.
async fn type_message(
    _person: super::guard::Person,
    State(state): State<Arc<Pinrail>>,
    Path((agent, id, message)): Path<(String, String, i64)>,
) -> Result<StatusCode, ApiError> {
    let found = state
        .reviews()
        .message(&agent, &id, message)?
        .ok_or_else(|| Error::NotFound(format!("message {message} of session {id}")))?;
    if !matches!(found.state.as_str(), "queued" | "unconfirmed") {
        return Err(Error::Conflict("This message is not waiting to be typed.".to_string()).into());
    }
    #[cfg(unix)]
    {
        if state.runtime().message_route(&agent, &id) != "type" {
            return Err(Error::Conflict(
                "Pinrail no longer runs this session, so it cannot type into it.".to_string(),
            )
            .into());
        }
        let app = state.clone();
        tokio::spawn(async move {
            let _ = app.runtime().type_now(&agent, &id, message).await;
        });
        Ok(StatusCode::ACCEPTED)
    }
    #[cfg(not(unix))]
    {
        Err(Error::Conflict("Pinrail does not run agents on this system.".to_string()).into())
    }
}

/// What `pinrail status` sends once it has printed the messages a status
/// handed over, naming the session by the agent's own id: their `ids`. They are delivered then; until then, a claim
/// that lapses hands them over again. Answers 204.
async fn acknowledge_messages(
    State(state): State<Arc<Pinrail>>,
    Path((agent, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<StatusCode, ApiError> {
    let ids: Vec<i64> = body
        .get("ids")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::invalid("ids", "list the messages' ids"))?
        .iter()
        .map(|v| {
            v.as_i64()
                .ok_or_else(|| Error::invalid("ids", "must be numbers"))
        })
        .collect::<Result<_, _>>()?;
    state.reviews().acknowledge_messages(&agent, &id, &ids)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Deletes a message the agent has not received yet. Answers 204, or 409
/// once the agent has received it.
async fn delete_message(
    _person: super::guard::Person,
    State(state): State<Arc<Pinrail>>,
    Path((agent, id, message)): Path<(String, String, i64)>,
) -> Result<StatusCode, ApiError> {
    state.reviews().delete_message(&agent, &id, message)?;
    Ok(StatusCode::NO_CONTENT)
}
