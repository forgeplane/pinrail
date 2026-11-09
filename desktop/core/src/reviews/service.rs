//! The reviews service: submit, read, list, decide, withdraw, discard,
//! wait, expire.
//!
//! Writes go to the database, then out on the bus. Status transitions are
//! `pending -> decided | withdrawn | discarded | expired` and nothing else;
//! the database enforces that with the primary key on `outcomes`, so a
//! review ends once, whichever way, and no two racing endings both win.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Datelike, Utc};
use serde_json::{Map, Value};

use super::model::{Decision, Kind, Review, Source, Via, parse_datetime};
use super::questions;
use crate::attachments::{Attachments, Presence, ReviewAttachment};
use crate::db::{
    Db, Deleted, Event, Filters, MessageRow, NotStored, OpenStage, SessionFilters, SessionRow,
    StageRow, StatusKind, StatusRow, delivered_notice, message_notice, status_notice,
};
use crate::error::{Error, Violation};
use crate::events::{self, Bus, Notice};
use crate::plugins::{Plugin, Registry};

const DEFAULT_LIMIT: usize = 100;
/// The longest status an agent can report, in characters.
const STATUS_MAX: usize = 200;
/// The longest note, in characters.
const NOTE_MAX: usize = 10_000;
/// The longest message to the agent, in characters.
const MESSAGE_MAX: usize = 4_000;

/// One page of a listing; see `Reviews::listing`.
#[derive(Debug)]
pub struct Listing {
    pub reviews: Vec<Review>,
    /// every review the filters match, whatever the cursor, offset or limit
    pub total: usize,
    /// whether a next page exists; `next_cursor` fetches it
    pub has_more: bool,
    pub next_cursor: Option<String>,
    /// the filter menus' values, when asked for
    pub facets: Option<crate::db::Facets>,
}
const MAX_LIMIT: usize = 500;

/// What an agent waiting on a review is told when the person deletes its
/// repository from the app.
const DELETED_REASON: &str = "The person deleted this repository from Pinrail, with its reviews.";
/// What an agent waiting on a review is told when the person deletes it.
const REVIEW_DELETED_REASON: &str = "The person deleted this review from Pinrail.";

#[derive(Debug, Clone)]
pub struct Reviews {
    db: Arc<Db>,
    registry: Arc<Registry>,
    bus: Bus,
    user: String,
    attachments: Attachments,
}

/// A submission that passed every check, ready to store.
struct Checked<'a> {
    attrs: &'a Map<String, Value>,
    kind: Kind,
    /// the plugin that renders it; none for questions
    plugin: Option<Arc<Plugin>>,
    payload: Value,
    attachments: Vec<ReviewAttachment>,
    /// the work items it belongs to
    items: Vec<crate::db::ItemTag>,
}

/// The refusal for a round that already has a newer round.
/// The page size a listing asks for, within the server's bounds: the
/// default when none is given.
fn clamp_limit(limit: usize) -> usize {
    match limit {
        0 => DEFAULT_LIMIT,
        n => n.min(MAX_LIMIT),
    }
}

fn already_revised(id: &str, newer: &str) -> String {
    format!("review {id} already has a newer round, {newer}; revise that one")
}

impl Reviews {
    pub(crate) fn new(
        db: Arc<Db>,
        registry: Arc<Registry>,
        bus: Bus,
        user: String,
        attachments: Attachments,
    ) -> Self {
        Reviews {
            db,
            registry,
            bus,
            user,
            attachments,
        }
    }

    /// Submits a review from a request body. `plugin` and `title` are
    /// required; `origin`, `payload`, `revises`, `expires_at` and
    /// `requested_by` are optional. Every failure is `invalid` with
    /// violations pointing into the body.
    pub fn submit(&self, body: &Value, actor: Option<&str>) -> Result<Review, Error> {
        self.submit_once(body, actor).map(|(review, _)| review)
    }

    /// The work items a new review belongs to: those `items` names, by
    /// their keys or their numbers in the origin's repository, none for an
    /// empty list; without `items`, those of the round it revises, or else
    /// every item the session holds a claim of.
    fn items_for(&self, attrs: &Map<String, Value>) -> Result<Vec<crate::db::ItemTag>, Error> {
        let ids: Vec<String> = match attrs.get("items") {
            Some(Value::Array(named)) => {
                let repo = attrs
                    .get("origin")
                    .and_then(|o| o.get("repo"))
                    .and_then(Value::as_str);
                let mut ids = Vec::new();
                for (i, named) in named.iter().enumerate() {
                    let path = format!("/items/{i}");
                    let Some(named) = named.as_str() else {
                        return Err(Error::invalid(
                            path,
                            "must be an item's key, such as PIN-42",
                        ));
                    };
                    let item = crate::items::ItemRef::parse(named)
                        .map_err(|e| Error::invalid(path.clone(), e))?;
                    match self.db.item(&item, repo)? {
                        Ok(found) if !ids.contains(&found.id) => ids.push(found.id),
                        Ok(_) => {}
                        Err(refused) => return Err(Error::invalid(path, refused.to_string())),
                    }
                }
                ids
            }
            Some(_) => {
                return Err(Error::invalid(
                    "/items",
                    "must be a list of item keys, such as [\"PIN-42\"]",
                ));
            }
            None => {
                let revised = attrs.get("revises").and_then(Value::as_str);
                let agent = attrs.get("requested_by").and_then(Value::as_str);
                let session = attrs.get("session").and_then(Value::as_str);
                match (revised, agent, session) {
                    (Some(revised), _, _) => self.db.items_of_review(revised)?,
                    (None, Some(agent), Some(session)) => self.db.session_claims(agent, session)?,
                    _ => Vec::new(),
                }
            }
        };
        let mut found = Vec::new();
        for id in ids {
            if let Ok(w) = self.db.item(&crate::items::ItemRef::Id(id), None)? {
                found.push(w);
            }
        }
        // in the order a read gives them
        found.sort_by(|a, b| (&a.key.prefix, a.key.number).cmp(&(&b.key.prefix, b.key.number)));
        Ok(found
            .into_iter()
            .map(|w| crate::db::ItemTag {
                id: w.id,
                key: w.key.to_string(),
                title: w.title,
            })
            .collect())
    }

    /// The bundle a new review records: the installed one, which for a
    /// linked plugin is its folder as it was captured when it was fetched.
    fn bundle_of(&self, plugin: &crate::plugins::Plugin) -> Result<String, Error> {
        match plugin.install.as_ref() {
            Some(install) => install
                .bundle
                .clone()
                .ok_or_else(|| Error::Internal(format!("{} has no bundle", plugin.name))),
            None => Err(Error::Internal(format!("{} is not installed", plugin.name))),
        }
    }

    /// As `submit`, and whether the review is new: a submission the same as
    /// a review still pending, as an agent that runs its command again
    /// sends, answers that review instead of creating a second one.
    pub fn submit_once(&self, body: &Value, actor: Option<&str>) -> Result<(Review, bool), Error> {
        let Checked {
            attrs,
            kind,
            plugin,
            payload,
            attachments,
            items,
        } = self.check(body, Presence::Stored)?;
        let summary = match &plugin {
            Some(plugin) => plugin
                .summary
                .request
                .as_ref()
                .and_then(|rules| rules.derive(&payload)),
            None => questions::request_summary(&payload),
        };
        let review = Review {
            id: crate::id::next(),
            kind,
            plugin: plugin.as_ref().map(|p| p.name.clone()).unwrap_or_default(),
            plugin_version: plugin
                .as_ref()
                .map(|p| p.version.clone())
                .unwrap_or_default(),
            plugin_bundle: plugin.as_deref().map(|p| self.bundle_of(p)).transpose()?,
            title: attrs["title"].as_str().unwrap_or_default().to_string(),
            origin: Review::normalize_origin(attrs.get("origin")),
            requested_by: attrs
                .get("requested_by")
                .and_then(Value::as_str)
                .map(str::to_string),
            session: attrs
                .get("session")
                .and_then(Value::as_str)
                .map(str::to_string),
            created_at: Utc::now(),
            expires_at: attrs
                .get("expires_at")
                .and_then(Value::as_str)
                .and_then(parse_datetime),
            revises: attrs
                .get("revises")
                .and_then(Value::as_str)
                .map(str::to_string),
            summary,
            payload: Some(payload),
            decision: None,
            agent_note: None,
            withdrawn_at: None,
            withdrawn_reason: None,
            withdrawn_by: None,
            withdrawn_via: None,
            withdrawn_session: None,
            discarded_at: None,
            discarded_by: None,
            discarded_reason: None,
            discarded_via: None,
            discarded_session: None,
            attachments_total: (
                attachments.len() as u64,
                attachments.iter().map(|a| a.size).sum(),
            ),
            attachments,
            starred_at: None,
            // a new round of a starred review is starred through its last
            starred: match attrs.get("revises").and_then(Value::as_str) {
                Some(last) => self.db.is_starred(last)?,
                None => false,
            },
            project: None,
            items,
        };
        // the notice and the answer name the project as a read would
        let mut review = review;
        if let Some(repo) = review.origin.get("repo").and_then(Value::as_str) {
            review.project = self.db.project_of_repo(repo)?;
        }
        let event_id = match self.db.insert_review_once(&review, actor) {
            Ok(Ok(event_id)) => event_id,
            Ok(Err(NotStored::Twin(existing))) => return Ok((self.get(&existing)?, false)),
            Ok(Err(NotStored::RevisesGone(id))) => {
                return Err(Error::invalid("/revises", format!("unknown review {id}")));
            }
            Ok(Err(NotStored::FileGone(name))) => {
                return Err(Error::invalid(
                    format!("/attachments/{name}/sha256"),
                    "not uploaded: PUT /api/v1/attachments/{sha256} first",
                ));
            }
            // another submission revised the same round a moment earlier
            // (the only unique index a new review can collide on)
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
            {
                let revised = review.revises.clone().unwrap_or_default();
                let newer = self.db.newer_round(&revised)?.unwrap_or_default();
                return Err(Error::invalid(
                    "/revises",
                    already_revised(&revised, &newer),
                ));
            }
            Err(e) => return Err(e.into()),
        };
        self.publish(event_id, events::CREATED, &review);
        // a round still waiting when a newer one arrives is superseded: its
        // waiting agent is told, and the inbox shows only the newer round
        if let Some(revised) = &review.revises {
            let reason = format!("superseded by {}", review.id);
            let by = review.requested_by.as_deref();
            match self.withdrawal(revised, Some(&reason), by, None, review.session.as_deref()) {
                Ok(_) | Err(Error::NotPending(_)) => {}
                Err(e) => return Err(e),
            }
        }
        Ok((review, true))
    }

    /// Runs every check a submission gets and stores nothing: the plugin
    /// that would render the review, none for questions, or the violations
    /// `submit` would give. Attachments are checked as described, not for
    /// being uploaded, so a dry run can come before the uploads.
    pub fn validate(&self, body: &Value) -> Result<Option<Arc<Plugin>>, Error> {
        self.check(body, Presence::Described)
            .map(|checked| checked.plugin)
    }

    fn check<'a>(&self, body: &'a Value, presence: Presence) -> Result<Checked<'a>, Error> {
        let Value::Object(attrs) = body else {
            return Err(Error::invalid("", "must be a JSON object"));
        };
        let kind = self.envelope_violations(attrs)?;
        let payload = attrs
            .get("payload")
            .cloned()
            .unwrap_or(Value::Object(Map::new()));
        let plugin = match kind {
            Kind::Review => {
                let plugin_name = attrs["plugin"].as_str().unwrap_or_default();
                self.capture(plugin_name)?;
                Some(self.registry.fetch(plugin_name)?)
            }
            Kind::Question => None,
        };
        let violations: Vec<Violation> = match &plugin {
            Some(plugin) => plugin.validate_payload(&payload),
            None => questions::check_payload(&payload),
        }
        .into_iter()
        .map(|v| Violation::new(format!("/payload{}", v.path), v.message))
        .collect();
        if !violations.is_empty() {
            return Err(Error::Invalid(violations));
        }
        let attachments = self.attachments.check(
            attrs.get("attachments"),
            plugin.as_ref().and_then(|p| p.attachments.as_ref()),
            &payload,
            presence,
        )?;
        let items = self.items_for(attrs)?;
        Ok(Checked {
            attrs,
            kind,
            plugin,
            payload,
            attachments,
            items,
        })
    }

    /// The sessions the filters admit, those waiting for the person first,
    /// and how many there are in all.
    pub fn sessions(&self, filters: &SessionFilters) -> Result<(Vec<SessionRow>, usize), Error> {
        Ok(self.db.sessions(filters, Utc::now())?)
    }

    /// The projects of all the sessions, and whether a session has none.
    pub fn session_projects(&self) -> Result<(Vec<String>, bool), Error> {
        Ok(self.db.session_projects()?)
    }

    /// A session and what was sent from it, oldest first: its questions
    /// with their payloads, which the thread shows, and its reviews without.
    pub fn session(&self, agent: &str, id: &str) -> Result<(SessionRow, Vec<Review>), Error> {
        let now = Utc::now();
        let Some(session) = self.db.session(agent, id, now)? else {
            return Err(Error::NotFound(format!("session {id} of {agent}")));
        };
        let filters = Filters {
            agent: Some(agent.to_string()),
            session: Some(id.to_string()),
            include_revised: true,
            oldest_first: true,
            ..Filters::default()
        };
        let items = self
            .db
            .list(&filters, now)?
            .into_iter()
            .map(|review| match review.kind {
                Kind::Question => self.get(&review.id),
                Kind::Review => Ok(review),
            })
            .collect::<Result<_, _>>()?;
        Ok((session, items))
    }

    /// Records what the agent says it is doing in its session: one line of
    /// text, which may be empty only when the agent says it has finished,
    /// or a note, Markdown of several lines. The start of a task names it,
    /// and so the session. Returns the status and the person's queued
    /// messages, which it hands to the agent.
    pub fn status(
        &self,
        agent: &str,
        session: &str,
        text: &str,
        kind: StatusKind,
        origin: Option<&Value>,
        attachments: Option<&Value>,
    ) -> Result<(Vec<MessageRow>, Option<OpenStage>), Error> {
        let text = text.trim();
        if agent.trim().is_empty() || session.trim().is_empty() {
            return Err(Error::invalid("session", "name the agent and the session"));
        }
        let note = kind == StatusKind::Note;
        if !note && text.contains(['\n', '\r']) {
            return Err(Error::invalid("text", "must be one line"));
        }
        let max = if note { NOTE_MAX } else { STATUS_MAX };
        if text.chars().count() > max {
            return Err(Error::invalid(
                "text",
                format!("must be at most {max} characters"),
            ));
        }
        match kind {
            StatusKind::Note if text.is_empty() => {
                return Err(Error::invalid("text", "write the note"));
            }
            StatusKind::Step if text.is_empty() => {
                return Err(Error::invalid("text", "say what the agent is doing"));
            }
            StatusKind::Start if text.is_empty() => {
                return Err(Error::invalid("text", "name the task the agent starts"));
            }
            _ => {}
        }
        // only a note carries files
        let attachments = if note {
            self.attachments
                .check_note(attachments, text, ("/text", "the note"))?
        } else if attachments.is_some_and(|a| !a.is_null()) {
            return Err(Error::invalid("attachments", "only a note carries files"));
        } else {
            Vec::new()
        };
        let origin = Review::normalize_origin(origin);
        let added = match self
            .db
            .add_status(agent, session, text, kind, &origin, &attachments)?
        {
            Ok(added) => added,
            Err(name) => {
                return Err(Error::invalid(
                    "attachments",
                    format!("{name} was deleted before the note was stored: upload it again"),
                ));
            }
        };
        self.bus.publish(Notice {
            event_id: added.event_id,
            kind: events::SESSION_STATUS.to_string(),
            review_id: None,
            review: None,
            keys: None,
            session: Some(status_notice(agent, session, &added.status)),
            item: None,
        });
        if let Some(event_id) = added.delivered {
            self.bus.publish(Notice {
                event_id,
                kind: events::SESSION_MESSAGES_DELIVERED.to_string(),
                review_id: None,
                review: None,
                keys: None,
                session: Some(delivered_notice(agent, session, &added.messages)),
                item: None,
            });
        }
        Ok((added.messages, added.stage))
    }

    /// Queues a message from the person to the agent of a session, which
    /// the agent receives with its next status.
    pub fn send_message(
        &self,
        agent: &str,
        session: &str,
        text: &str,
    ) -> Result<MessageRow, Error> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::invalid("text", "write the message"));
        }
        if text.chars().count() > MESSAGE_MAX {
            return Err(Error::invalid(
                "text",
                format!("must be at most {MESSAGE_MAX} characters"),
            ));
        }
        let Some((message, event_id)) = self.db.add_message(agent, session, text, &self.user)?
        else {
            return Err(Error::NotFound(format!("session {session} of {agent}")));
        };
        self.bus.publish(Notice {
            event_id,
            kind: events::SESSION_MESSAGE_SENT.to_string(),
            review_id: None,
            review: None,
            keys: None,
            session: Some(message_notice(agent, session, &message)),
            item: None,
        });
        Ok(message)
    }

    /// Deletes a message the agent has not received yet.
    pub fn delete_message(&self, agent: &str, session: &str, message: i64) -> Result<(), Error> {
        match self.db.delete_message(agent, session, message)? {
            Deleted::Yes(event_id, deleted) => {
                self.bus.publish(Notice {
                    event_id,
                    kind: events::SESSION_MESSAGE_DELETED.to_string(),
                    review_id: None,
                    review: None,
                    keys: None,
                    session: Some(message_notice(agent, session, &deleted)),
                    item: None,
                });
                Ok(())
            }
            Deleted::Delivered => Err(Error::Conflict(
                "The agent has already received this message.".to_string(),
            )),
            Deleted::Missing => Err(Error::NotFound(format!(
                "message {message} of session {session}"
            ))),
        }
    }

    /// The session's messages to the agent, oldest first.
    pub fn messages(&self, agent: &str, session: &str) -> Result<Vec<MessageRow>, Error> {
        Ok(self.db.messages(agent, session)?)
    }

    /// The session's statuses, oldest first.
    /// Archives a session, or brings it back. Archiving an archived session
    /// keeps its first time.
    pub fn archive_session(&self, agent: &str, id: &str, archived: bool) -> Result<(), Error> {
        let Some(event_id) = self.db.set_archived(agent, id, archived.then(Utc::now))? else {
            return Err(Error::NotFound(format!("session {id} of {agent}")));
        };
        let kind = if archived {
            events::SESSION_ARCHIVED
        } else {
            events::SESSION_UNARCHIVED
        };
        self.bus.publish(Notice {
            event_id,
            kind: kind.to_string(),
            review_id: None,
            review: None,
            keys: None,
            session: Some(serde_json::json!({ "agent": agent, "id": id })),
            item: None,
        });
        Ok(())
    }

    /// The stages agents started whose names hold every word of `text`,
    /// newest first.
    pub fn stages(&self, text: Option<&str>, limit: usize) -> Result<Vec<StageRow>, Error> {
        Ok(self.db.stages(text, limit)?)
    }

    pub fn statuses(&self, agent: &str, session: &str) -> Result<Vec<StatusRow>, Error> {
        Ok(self.db.statuses(agent, session)?)
    }

    pub fn get(&self, id: &str) -> Result<Review, Error> {
        self.db
            .get_review(id)?
            .ok_or_else(|| Error::NotFound(format!("review {id}")))
    }

    pub fn list(&self, filters: &Filters) -> Result<Vec<Review>, Error> {
        let mut filters = filters.clone();
        filters.limit = clamp_limit(filters.limit);
        Ok(self.db.list(&filters, Utc::now())?)
    }

    /// One page of a listing, newest first unless the filters ask for the
    /// oldest: the reviews, how many match in
    /// all, whether there is more and the cursor to it, and, when asked, the
    /// values the filter menus can offer. A page follows from `cursor` (the
    /// way to walk everything) or from `offset` (numbered pages).
    pub fn listing(&self, filters: &Filters, facets: bool) -> Result<Listing, Error> {
        let now = Utc::now();
        let mut filters = filters.clone();
        filters.limit = clamp_limit(filters.limit);
        let limit = filters.limit;
        // one row past the page says whether there is another
        filters.limit = limit + 1;
        let mut reviews = self.db.list(&filters, now)?;
        let has_more = reviews.len() > limit;
        reviews.truncate(limit);
        Ok(Listing {
            next_cursor: has_more
                .then(|| reviews.last().map(|r| r.id.clone()))
                .flatten(),
            has_more,
            total: self.db.count(&filters, now)?,
            facets: if facets {
                Some(self.db.facets(&filters, now)?)
            } else {
                None
            },
            reviews,
        })
    }

    pub fn rounds(&self, id: &str) -> Result<Vec<Review>, Error> {
        let rounds = self.db.rounds(id)?;
        if rounds.is_empty() {
            return Err(Error::NotFound(format!("review {id}")));
        }
        Ok(rounds)
    }

    pub fn events(&self, id: &str) -> Result<Vec<Event>, Error> {
        if !self.db.exists(id)? {
            return Err(Error::NotFound(format!("review {id}")));
        }
        Ok(self.db.events_for(id)?)
    }

    /// Records the decision, made by the configured user. `data` is validated
    /// against the decision schema of the plugin version the review was
    /// submitted under.
    /// Records the person's decision, checked by the version of the plugin
    /// the review records. `shown` is the bundle the person's view showed:
    /// when the review has moved to another since, the decision is refused
    /// as a conflict, to be made again on the review as it is.
    pub fn decide(
        &self,
        id: &str,
        data: &Value,
        agent_note: Option<&str>,
        shown: Option<&str>,
        source: &Source,
    ) -> Result<Review, Error> {
        let review = self.get(id)?;
        if !review.is_pending(Utc::now()) {
            return Err(Error::NotPending(id.to_string()));
        }
        let moved = |id: &str| {
            Error::Conflict(format!(
                "review {id} moved to another version of its plugin; open it again"
            ))
        };
        if shown.is_some() && shown != review.plugin_bundle.as_deref() {
            return Err(moved(id));
        }
        let summary = self.judge(&review, data)?;
        let decision = Decision {
            decided_by: self.attributed(source),
            decided_at: Utc::now(),
            data: data.clone(),
            summary,
            via: Some(source.via),
            session: source.session.clone(),
        };
        let note = agent_note.map(str::trim).filter(|n| !n.is_empty());
        let Some(event_id) =
            self.db
                .insert_decision(id, &decision, note, review.plugin_bundle.as_deref())?
        else {
            // ended meanwhile, or moved by an opening of the review
            return Err(match self.get(id)?.is_pending(Utc::now()) {
                true => moved(id),
                false => Error::NotPending(id.to_string()),
            });
        };
        let review = self.get(id)?;
        self.publish(event_id, events::DECIDED, &review);
        Ok(review)
    }

    /// A review as the app opens it, with the plugin version it renders
    /// with. A pending review moves first to the version of its plugin
    /// installed now, a linked folder as it is now, when that version takes
    /// its payload and its files; when it does not, the review stays, and
    /// `refused` says why. An ended review keeps its version.
    pub fn open(&self, id: &str) -> Result<Opened, Error> {
        let review = self.get(id)?;
        if review.kind == Kind::Question {
            return Err(Error::NotFound(format!(
                "a view for {id}: questions are shown with the app's own form"
            )));
        }
        let mut refused = None;
        if review.is_pending(Utc::now()) {
            self.capture(&review.plugin)?;
            if let Some(installed) = self.registry.get(&review.plugin).filter(|p| p.usable())
                && let Some(bundle) = installed.install.as_ref().and_then(|i| i.bundle.clone())
                && review.plugin_bundle.as_deref() != Some(bundle.as_str())
            {
                match self.takes(&installed, &review) {
                    Ok(summary) => {
                        let attrs = serde_json::json!({
                            "from": review.plugin_version,
                            "to": installed.version,
                        });
                        if let Some(event_id) = self.db.move_review(
                            id,
                            review.plugin_bundle.as_deref(),
                            &bundle,
                            &installed.version,
                            summary.as_ref(),
                            &attrs,
                        )? {
                            self.publish(event_id, events::PLUGIN_CHANGED, &self.get(id)?);
                        }
                    }
                    Err(why) => {
                        refused = Some(format!(
                            "{} {} does not take this review: {why}",
                            installed.name, installed.version
                        ))
                    }
                }
            }
        }
        let review = self.get(id)?;
        let plugin = self
            .registry
            .fetch_review(&review.plugin, review.plugin_bundle.as_deref())?;
        let installed = self
            .registry
            .get(&review.plugin)
            .and_then(|p| p.install.as_ref()?.bundle.clone());
        Ok(Opened {
            review,
            plugin,
            refused,
            installed,
        })
    }

    /// Whether `plugin` takes a review's payload and files, as a submission
    /// to it would be checked: the request summary it would give, or why not.
    fn takes(&self, plugin: &Plugin, review: &Review) -> Result<Option<Value>, String> {
        let payload = review.payload.clone().unwrap_or(Value::Object(Map::new()));
        let declared: Map<String, Value> = review
            .attachments
            .iter()
            .map(|a| {
                (
                    a.name.clone(),
                    serde_json::json!({ "sha256": a.sha256, "size": a.size, "media_type": a.media_type }),
                )
            })
            .collect();
        let mut violations: Vec<Violation> = plugin
            .validate_payload(&payload)
            .into_iter()
            .map(|v| Violation::new(format!("/payload{}", v.path), v.message))
            .collect();
        if let Err(Error::Invalid(more)) = self.attachments.check(
            (!declared.is_empty()).then_some(&Value::Object(declared)),
            plugin.attachments.as_ref(),
            &payload,
            Presence::Stored,
        ) {
            violations.extend(more);
        }
        if !violations.is_empty() {
            return Err(violations
                .iter()
                .map(|v| format!("{}: {}", v.path, v.message))
                .collect::<Vec<_>>()
                .join("; "));
        }
        Ok(plugin
            .summary
            .request
            .as_ref()
            .and_then(|rules| rules.derive(&payload)))
    }

    /// What `decide` would say of a decision, deciding nothing: the
    /// review is there and the data passes its plugin's decision schema,
    /// or answers its questions.
    pub fn check_decision(&self, id: &str, data: &Value) -> Result<(), Error> {
        let review = self.get(id)?;
        self.judge(&review, data).map(|_| ())
    }

    /// Checks a decision on `review`, by the plugin version it records or
    /// against its questions, and gives the decision's summary.
    fn judge(&self, review: &Review, data: &Value) -> Result<Option<Value>, Error> {
        let (violations, summary) = match review.kind {
            Kind::Question => {
                let payload = review.payload.as_ref().unwrap_or(&Value::Null);
                (
                    questions::check_decision(payload, data),
                    questions::outcome_summary(data),
                )
            }
            Kind::Review => {
                let plugin = self
                    .registry
                    .fetch_review(&review.plugin, review.plugin_bundle.as_deref())?;
                let summary = plugin
                    .summary
                    .outcome
                    .as_ref()
                    .and_then(|rules| rules.derive(data));
                (plugin.validate_decision(data), summary)
            }
        };
        if violations.is_empty() {
            Ok(summary)
        } else {
            Err(Error::Invalid(violations))
        }
    }

    /// The requester gives up.
    pub fn withdraw(
        &self,
        id: &str,
        reason: Option<&str>,
        source: &Source,
    ) -> Result<Review, Error> {
        let by = self.attributed(source);
        self.withdrawal(
            id,
            reason,
            by.as_deref(),
            Some(source.via),
            source.session.as_deref(),
        )
    }

    fn withdrawal(
        &self,
        id: &str,
        reason: Option<&str>,
        by: Option<&str>,
        via: Option<Via>,
        session: Option<&str>,
    ) -> Result<Review, Error> {
        let review = self.get(id)?;
        if !review.is_pending(Utc::now()) {
            return Err(Error::NotPending(id.to_string()));
        }
        let reason = reason.map(str::trim).filter(|r| !r.is_empty());
        let Some(event_id) = self
            .db
            .insert_withdrawal(id, Utc::now(), by, reason, via, session)?
        else {
            return Err(Error::NotPending(id.to_string()));
        };
        let review = self.get(id)?;
        self.publish(event_id, events::WITHDRAWN, &review);
        Ok(review)
    }

    /// The person's "no, and stop": nothing is decided, the review leaves
    /// the inbox, and the agent waiting on it is told, reason included.
    pub fn discard(
        &self,
        id: &str,
        reason: Option<&str>,
        source: &Source,
    ) -> Result<Review, Error> {
        let review = self.get(id)?;
        if !review.is_pending(Utc::now()) {
            return Err(Error::NotPending(id.to_string()));
        }
        let by = self.attributed(source);
        let by = by.as_deref();
        let reason = reason.map(str::trim).filter(|r| !r.is_empty());
        let Some(event_id) = self.db.insert_discard(id, Utc::now(), by, reason, source)? else {
            return Err(Error::NotPending(id.to_string()));
        };
        let review = self.get(id)?;
        self.publish(event_id, events::DISCARDED, &review);
        Ok(review)
    }

    /// Who a decision, a discard or a withdrawal is recorded as made by: the person, the
    /// server's user, when it came from the app; otherwise only whoever the
    /// client named, since nothing says a person was there.
    fn attributed(&self, source: &Source) -> Option<String> {
        match source.via {
            Via::App => Some(self.user.clone()),
            Via::Cli | Via::Api => source
                .by
                .as_deref()
                .map(str::trim)
                .filter(|b| !b.is_empty())
                .map(str::to_string),
        }
    }

    /// Logs that a person opened the review and announces it, so the
    /// notification that announced the review can close.
    /// Stars a round of a review, or removes its star. Starring a starred
    /// round again keeps its first time. Questions cannot be starred.
    pub fn star(&self, id: &str, starred: bool) -> Result<(), Error> {
        let review = self.get(id)?;
        if review.kind == Kind::Question {
            return Err(Error::invalid("/kind", "a question cannot be starred"));
        }
        self.db.set_star(id, starred.then(Utc::now))?;
        let kind = if starred {
            events::STARRED
        } else {
            events::UNSTARRED
        };
        let event_id = self
            .db
            .append_event(Some(id), kind, Some(&self.user), &Value::Null)?;
        self.publish(event_id, kind, &self.get(id)?);
        Ok(())
    }

    pub fn mark_viewed(&self, id: &str) -> Result<(), Error> {
        if self.db.exists(id)? {
            let event_id =
                self.db
                    .append_event(Some(id), events::VIEWED, Some(&self.user), &Value::Null)?;
            self.publish(event_id, events::VIEWED, &self.get(id)?);
        }
        Ok(())
    }

    /// Finds reviews whose expiry passed without a decision, logs `expired`
    /// once for each and notifies. Status is derived, so they already read as
    /// expired; this makes waiters and the inbox notice.
    pub fn sweep_expired(&self) -> Result<Vec<Review>, Error> {
        let expired = self.db.newly_expired(Utc::now())?;
        for review in &expired {
            let event_id =
                self.db
                    .append_event(Some(&review.id), events::EXPIRED, None, &Value::Null)?;
            self.db
                .item_review_expired(&review.id, review.expires_at.unwrap_or_else(Utc::now))?;
            self.publish(event_id, events::EXPIRED, review);
        }
        Ok(expired)
    }

    /// Deletes the reviews that ended more than `keep_days` ago, with their
    /// events and outcomes, and the plugin lines nothing renders with any
    /// more. `None` keeps everything, and so does 0, which the settings
    /// refuse. Returns how many reviews went.
    pub fn sweep_history(&self, keep_days: Option<u32>) -> Result<usize, Error> {
        // a limit too far back to subtract from today keeps everything
        let before = keep_days
            .filter(|days| *days > 0)
            .and_then(|days| chrono::Duration::try_days(days.into()))
            .and_then(|kept| Utc::now().checked_sub_signed(kept));
        match before {
            Some(before) => self.sweep_history_before(before),
            None => Ok(0),
        }
    }

    /// Deletes a repository from the app, as the person asked: what waits
    /// from it is discarded first, so the agents waiting on it are told to
    /// stop, then every round of its reviews and its sessions are deleted,
    /// and it leaves its project. Returns what went.
    pub fn delete_repo(&self, repo: &str, source: &Source) -> Result<Value, Error> {
        let repo = repo.trim();
        if repo.is_empty() {
            return Err(Error::invalid("/repo", "is empty"));
        }
        let mut discarded = 0;
        for id in self.db.pending_in_repo(repo, Utc::now())? {
            match self.discard(&id, Some(DELETED_REASON), source) {
                Ok(_) => discarded += 1,
                // decided or withdrawn meanwhile: it goes with the rest
                Err(Error::NotPending(_) | Error::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        let (reviews, sessions, items, event_id) = self.db.delete_repo(repo)?;
        self.publish_plain(event_id, events::REPOSITORY_DELETED);
        Ok(serde_json::json!({
            "repo": repo,
            "reviews": reviews,
            "sessions": sessions,
            "items": items,
            "discarded": discarded,
        }))
    }

    /// Deletes a review with every round of it, as the person asked: a
    /// round still waiting is discarded first, so the agent waiting on it
    /// is told to stop. Returns the ids that went.
    pub fn delete_review(&self, id: &str, source: &Source) -> Result<Vec<String>, Error> {
        let ids: Vec<String> = self.rounds(id)?.into_iter().map(|r| r.id).collect();
        for round in &ids {
            match self.discard(round, Some(REVIEW_DELETED_REASON), source) {
                Ok(_) | Err(Error::NotPending(_) | Error::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        self.db.delete_reviews(&refs)?;
        let event_id = self.db.append_event(
            None,
            events::REVIEW_DELETED,
            None,
            &serde_json::json!({ "ids": ids }),
        )?;
        self.publish_plain(event_id, events::REVIEW_DELETED);
        Ok(ids)
    }

    /// `sweep_history` with the moment spelled out.
    pub fn sweep_history_before(&self, before: DateTime<Utc>) -> Result<usize, Error> {
        let ended = self.db.ended_before(before)?;
        let ids: Vec<&str> = ended.iter().map(|(id, _, _)| id.as_str()).collect();
        let count = if ids.is_empty() {
            0
        } else {
            self.db.delete_reviews(&ids)?
        };
        // and the sessions that hold nothing more, and did nothing since,
        // with the claims they held
        let (_, released) = self.db.delete_idle_sessions(before)?;
        if count == 0 && released == 0 {
            return Ok(0);
        }
        // the bundles only these reviews kept go with the bundles' sweep
        let event_id = self.db.append_event(
            None,
            events::HISTORY_SWEPT,
            None,
            &serde_json::json!({ "count": count, "released": released }),
        )?;
        self.publish_plain(event_id, events::HISTORY_SWEPT);
        Ok(count)
    }

    /// Blocks until the review leaves pending, or `timeout` passes.
    pub async fn wait(&self, id: &str, timeout: Duration) -> Result<Option<Review>, Error> {
        let mut rx = self.bus.subscribe();
        let deadline = tokio::time::Instant::now() + timeout;
        let mut review = self.get(id)?;
        loop {
            let now = Utc::now();
            if !review.is_pending(now) {
                return Ok(Some(review));
            }
            let until_expiry = review.expires_at.map(|at| {
                let ms = (at - now).num_milliseconds().max(0) as u64 + 50;
                Duration::from_millis(ms)
            });
            let wake = match until_expiry {
                Some(d) => deadline.min(tokio::time::Instant::now() + d),
                None => deadline,
            };
            if wake <= tokio::time::Instant::now() && tokio::time::Instant::now() >= deadline {
                return Ok(None);
            }
            tokio::select! {
                notice = rx.recv() => {
                    match notice {
                        Ok(n) if n.review_id.as_deref() == Some(id) => match self.get(id) {
                            Ok(read) => review = read,
                            // discarded and deleted at once, as the waiting
                            // rounds of a deleted review or repository are:
                            // the waiter is told of the discard it was sent
                            Err(Error::NotFound(_)) if n.kind == events::DISCARDED => {
                                return Ok(Some(discarded_from(review, &n)));
                            }
                            Err(e) => return Err(e),
                        },
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => review = self.get(id)?,
                        Err(_) => return Ok(None),
                    }
                }
                _ = tokio::time::sleep_until(wake) => {
                    review = self.get(id)?;
                    if tokio::time::Instant::now() >= deadline && review.is_pending(Utc::now()) {
                        return Ok(None);
                    }
                }
            }
        }
    }

    /// Stores a linked plugin's folder when it changed, and tells the app
    /// its plugins changed.
    fn capture(&self, plugin: &str) -> Result<(), Error> {
        if self.registry.capture(plugin)? {
            let event_id =
                self.db
                    .append_event(None, events::PLUGINS_RELOADED, None, &Value::Null)?;
            self.publish_plain(event_id, events::PLUGINS_RELOADED);
        }
        Ok(())
    }

    fn publish_plain(&self, event_id: i64, kind: &str) {
        self.bus.publish(Notice {
            event_id,
            kind: kind.to_string(),
            review_id: None,
            review: None,
            keys: None,
            session: None,
            item: None,
        });
    }

    fn publish(&self, event_id: i64, kind: &str, review: &Review) {
        self.bus.publish(Notice {
            event_id,
            kind: kind.to_string(),
            review_id: Some(review.id.clone()),
            review: Some(review.to_json(false)),
            keys: None,
            session: None,
            item: None,
        });
    }

    /// Envelope checks that run before the plugin's schema, all at once.
    /// The envelope's violations, or the kind of review it submits.
    fn envelope_violations(&self, attrs: &Map<String, Value>) -> Result<Kind, Error> {
        let mut violations = Vec::new();
        let kind = match attrs.get("kind") {
            None | Some(Value::Null) => Some(Kind::Review),
            Some(value) => value.as_str().and_then(Kind::parse),
        };
        // an unknown kind is said alone: whether a plugin is needed depends on it
        let required: &[&str] = match kind {
            Some(Kind::Review) => &["plugin", "title"],
            Some(Kind::Question) => &["title"],
            None => {
                violations.push(Violation::new("/kind", "must be review or question"));
                &["title"]
            }
        };
        let kind = kind.unwrap_or_default();
        for key in required {
            match attrs.get(*key) {
                Some(Value::String(s)) if !s.is_empty() => {}
                _ => violations.push(Violation::new(format!("/{key}"), "is required")),
            }
        }
        if kind == Kind::Question {
            if attrs.get("plugin").is_some_and(|p| !p.is_null()) {
                violations.push(Violation::new(
                    "/plugin",
                    "is not taken by questions, which the app shows with its own form",
                ));
            }
            if attrs.get("revises").is_some_and(|r| !r.is_null()) {
                violations.push(Violation::new(
                    "/revises",
                    "is not taken by questions; withdraw them and ask again",
                ));
            }
        }
        match attrs.get("expires_at") {
            None | Some(Value::Null) => {}
            // stored as text and compared as text, so only years that are
            // written with four digits in UTC compare as the times they are
            Some(Value::String(s))
                if parse_datetime(s).is_some_and(|at| (1..=9999).contains(&at.year())) => {}
            Some(_) => violations.push(Violation::new(
                "/expires_at",
                "must be an ISO 8601 datetime between the years 1 and 9999 in UTC",
            )),
        }
        // the rounds of a review form one line, all with one plugin: a new
        // round revises the latest round, never one that has a newer round
        match attrs.get("revises") {
            None | Some(Value::Null) => {}
            Some(_) if kind == Kind::Question => {}
            Some(Value::String(id)) => match self.db.plugin_of(id)? {
                None => {
                    violations.push(Violation::new("/revises", format!("unknown review {id}")));
                }
                Some(revised) => {
                    if let Some(plugin) = attrs.get("plugin").and_then(Value::as_str)
                        && plugin != revised
                    {
                        violations.push(Violation::new(
                            "/revises",
                            format!(
                                "review {id} uses the {revised} plugin; a new round must use the same plugin"
                            ),
                        ));
                    }
                    if let Some(newer) = self.db.newer_round(id)? {
                        violations.push(Violation::new("/revises", already_revised(id, &newer)));
                    }
                }
            },
            Some(_) => violations.push(Violation::new("/revises", "must be a review id")),
        }
        // the plugin sums a review up; an agent's summary is refused, not
        // dropped unseen
        if attrs.contains_key("summary") {
            violations.push(Violation::new(
                "/summary",
                "summary is not accepted; the plugin derives it from the payload",
            ));
        }
        match attrs.get("session") {
            None | Some(Value::Null) => {}
            Some(Value::String(s)) if s.len() <= 200 => {}
            Some(_) => violations.push(Violation::new(
                "/session",
                "must be a string of at most 200 characters",
            )),
        }
        for key in ["origin", "payload"] {
            if let Some(v) = attrs.get(key)
                && !v.is_null()
                && !v.is_object()
            {
                violations.push(Violation::new(format!("/{key}"), "must be an object"));
            }
        }
        // the app shows origin.url as a link, so it must be a web address
        if let Some(Value::String(url)) = attrs.get("origin").and_then(|o| o.get("url")) {
            let lower = url.trim().to_ascii_lowercase();
            if !lower.is_empty() && !lower.starts_with("http://") && !lower.starts_with("https://")
            {
                violations.push(Violation::new(
                    "/origin/url",
                    "must be an http or https URL",
                ));
            }
        }
        if violations.is_empty() {
            Ok(kind)
        } else {
            Err(Error::Invalid(violations))
        }
    }
}

/// A review as the app opens it: the review, the plugin version it renders
/// with, and, when the version installed now does not take it, why.
#[derive(Debug)]
pub struct Opened {
    pub review: Review,
    pub plugin: Arc<Plugin>,
    pub refused: Option<String>,
    /// the bundle its plugin's installation holds now
    pub installed: Option<String>,
}

/// The review as its discard's notice describes it, for a waiter whose
/// review was deleted right after, with its repository or by itself: the
/// person discarded it, when and why.
fn discarded_from(mut review: Review, notice: &Notice) -> Review {
    let field = |key: &str| {
        notice
            .review
            .as_ref()
            .and_then(|r| r.get(key))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    review.discarded_at = Some(
        field("discarded_at")
            .and_then(|at| parse_datetime(&at))
            .unwrap_or_else(Utc::now),
    );
    review.discarded_by = field("discarded_by");
    review.discarded_reason = field("discarded_reason");
    review
}
