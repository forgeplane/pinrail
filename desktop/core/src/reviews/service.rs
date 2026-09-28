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

use super::model::{Decision, Review, parse_datetime};
use crate::attachments::{Attachments, Presence, ReviewAttachment};
use crate::db::{Db, Event, Filters, NotStored};
use crate::error::{Error, Violation};
use crate::events::{self, Bus, Notice};
use crate::plugins::{Plugin, Registry};

const DEFAULT_LIMIT: usize = 100;

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
    plugin: Arc<Plugin>,
    payload: Value,
    attachments: Vec<ReviewAttachment>,
}

/// The refusal for a round that already has a newer round.
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
    /// required; `origin`, `payload`, `summary`, `revises`, `expires_at` and
    /// `requested_by` are optional. Every failure is `invalid` with
    /// violations pointing into the body.
    pub fn submit(&self, body: &Value, actor: Option<&str>) -> Result<Review, Error> {
        self.submit_once(body, actor).map(|(review, _)| review)
    }

    /// As `submit`, and whether the review is new: a submission the same as
    /// a review still pending, as an agent that runs its command again
    /// sends, answers that review instead of creating a second one.
    pub fn submit_once(&self, body: &Value, actor: Option<&str>) -> Result<(Review, bool), Error> {
        let Checked {
            attrs,
            plugin,
            payload,
            attachments,
        } = self.check(body, Presence::Stored)?;
        let review = Review {
            id: crate::id::next(),
            plugin: plugin.name.clone(),
            plugin_version: plugin.version,
            plugin_release: plugin.release.clone(),
            title: attrs["title"].as_str().unwrap_or_default().to_string(),
            origin: Review::normalize_origin(attrs.get("origin")),
            requested_by: attrs
                .get("requested_by")
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
            summary: attrs.get("summary").cloned().filter(|s| !s.is_null()),
            payload: Some(payload),
            decision: None,
            agent_note: None,
            withdrawn_at: None,
            withdrawn_reason: None,
            discarded_at: None,
            discarded_by: None,
            discarded_reason: None,
            attachments_total: (
                attachments.len() as u64,
                attachments.iter().map(|a| a.size).sum(),
            ),
            attachments,
        };
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
            match self.withdraw(revised, Some(&reason)) {
                Ok(_) | Err(Error::NotPending(_)) => {}
                Err(e) => return Err(e),
            }
        }
        Ok((review, true))
    }

    /// Runs every check a submission gets and stores nothing: the plugin
    /// that would render the review, or the violations `submit` would give.
    /// Attachments are checked as described, not for being uploaded, so a
    /// dry run can come before the uploads.
    pub fn validate(&self, body: &Value) -> Result<Arc<Plugin>, Error> {
        self.check(body, Presence::Described)
            .map(|checked| checked.plugin)
    }

    fn check<'a>(&self, body: &'a Value, presence: Presence) -> Result<Checked<'a>, Error> {
        let Value::Object(attrs) = body else {
            return Err(Error::invalid("", "must be a JSON object"));
        };
        self.envelope_violations(attrs)?;

        let plugin_name = attrs["plugin"].as_str().unwrap_or_default();
        let plugin = self.registry.fetch(plugin_name)?;
        let payload = attrs
            .get("payload")
            .cloned()
            .unwrap_or(Value::Object(Map::new()));
        let violations: Vec<Violation> = plugin
            .validate_payload(&payload)
            .into_iter()
            .map(|v| Violation::new(format!("/payload{}", v.path), v.message))
            .collect();
        if !violations.is_empty() {
            return Err(Error::Invalid(violations));
        }
        let attachments = self.attachments.check(
            attrs.get("attachments"),
            plugin.attachments.as_ref(),
            &payload,
            presence,
        )?;
        Ok(Checked {
            attrs,
            plugin,
            payload,
            attachments,
        })
    }

    pub fn get(&self, id: &str) -> Result<Review, Error> {
        self.db
            .get_review(id)?
            .ok_or_else(|| Error::NotFound(format!("review {id}")))
    }

    pub fn list(&self, filters: &Filters) -> Result<Vec<Review>, Error> {
        let mut filters = filters.clone();
        filters.limit = match filters.limit {
            0 => DEFAULT_LIMIT,
            n => n.min(MAX_LIMIT),
        };
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
        filters.limit = match filters.limit {
            0 => DEFAULT_LIMIT,
            n => n.min(MAX_LIMIT),
        };
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
    pub fn decide(
        &self,
        id: &str,
        data: &Value,
        agent_note: Option<&str>,
    ) -> Result<Review, Error> {
        let review = self.get(id)?;
        if !review.is_pending(Utc::now()) {
            return Err(Error::NotPending(id.to_string()));
        }
        let plugin = self
            .registry
            .fetch_version(&review.plugin, review.plugin_version)?;
        let violations = plugin.validate_decision(data);
        if !violations.is_empty() {
            return Err(Error::Invalid(violations));
        }
        let decision = Decision {
            decided_by: self.user.clone(),
            decided_at: Utc::now(),
            data: data.clone(),
        };
        let note = agent_note.map(str::trim).filter(|n| !n.is_empty());
        let Some(event_id) = self.db.insert_decision(id, &decision, note)? else {
            return Err(Error::NotPending(id.to_string()));
        };
        let review = self.get(id)?;
        self.publish(event_id, events::DECIDED, &review);
        Ok(review)
    }

    /// What `decide` would say of a decision, deciding nothing: the
    /// review is there and the data passes its plugin's decision schema.
    pub fn check_decision(&self, id: &str, data: &Value) -> Result<(), Error> {
        let review = self.get(id)?;
        let plugin = self
            .registry
            .fetch_version(&review.plugin, review.plugin_version)?;
        let violations = plugin.validate_decision(data);
        if violations.is_empty() {
            Ok(())
        } else {
            Err(Error::Invalid(violations))
        }
    }

    /// The requester gives up.
    pub fn withdraw(&self, id: &str, reason: Option<&str>) -> Result<Review, Error> {
        let review = self.get(id)?;
        if !review.is_pending(Utc::now()) {
            return Err(Error::NotPending(id.to_string()));
        }
        let reason = reason.map(str::trim).filter(|r| !r.is_empty());
        let Some(event_id) = self.db.insert_withdrawal(id, Utc::now(), reason)? else {
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
        by: Option<&str>,
        reason: Option<&str>,
    ) -> Result<Review, Error> {
        let review = self.get(id)?;
        if !review.is_pending(Utc::now()) {
            return Err(Error::NotPending(id.to_string()));
        }
        let by = by
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .unwrap_or(&self.user);
        let reason = reason.map(str::trim).filter(|r| !r.is_empty());
        let Some(event_id) = self.db.insert_discard(id, Utc::now(), by, reason)? else {
            return Err(Error::NotPending(id.to_string()));
        };
        let review = self.get(id)?;
        self.publish(event_id, events::DISCARDED, &review);
        Ok(review)
    }

    /// Logs that a person opened the review.
    pub fn mark_viewed(&self, id: &str) -> Result<(), Error> {
        if self.db.exists(id)? {
            self.db
                .append_event(Some(id), events::VIEWED, Some(&self.user), &Value::Null)?;
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
            self.publish(event_id, events::EXPIRED, review);
        }
        Ok(expired)
    }

    /// Deletes the reviews that ended more than `keep_days` ago, with their
    /// events and outcomes, and the store entries nothing renders from
    /// any more. `None` keeps everything, and so does 0, which the settings
    /// refuse. Returns how many reviews went.
    pub fn sweep_history(&self, keep_days: Option<u32>) -> Result<usize, Error> {
        match keep_days {
            Some(days) if days > 0 => {
                self.sweep_history_before(Utc::now() - chrono::Duration::days(days as i64))
            }
            _ => Ok(0),
        }
    }

    /// `sweep_history` with the moment spelled out.
    pub fn sweep_history_before(&self, before: DateTime<Utc>) -> Result<usize, Error> {
        let ended = self.db.ended_before(before)?;
        if ended.is_empty() {
            return Ok(0);
        }
        let ids: Vec<&str> = ended.iter().map(|(id, _, _)| id.as_str()).collect();
        let count = self.db.delete_reviews(&ids)?;
        // a store entry kept past the plugin's removal goes once nothing
        // renders from it
        let versions: std::collections::BTreeSet<(String, u32)> = ended
            .into_iter()
            .map(|(_, plugin, version)| (plugin, version))
            .collect();
        // under the lock installs take, so an install placing the same
        // entry again cannot land between the check and the deletion
        let _changing = self.registry.changing();
        let records = self.registry.records();
        for (plugin, version) in versions {
            if self.db.reviews_use(&plugin, version)? {
                continue;
            }
            if !records.iter().any(|r| r.name == plugin) {
                let _ = std::fs::remove_dir_all(self.registry.store_entry(&plugin, version as i64));
                let _ = std::fs::remove_dir(self.registry.store_dir().join(&plugin));
            }
        }
        let event_id = self.db.append_event(
            None,
            events::HISTORY_SWEPT,
            None,
            &serde_json::json!({ "count": count }),
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
                        Ok(n) if n.review_id.as_deref() == Some(id) => review = self.get(id)?,
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

    fn publish_plain(&self, event_id: i64, kind: &str) {
        self.bus.publish(Notice {
            event_id,
            kind: kind.to_string(),
            review_id: None,
            review: None,
            keys: None,
        });
    }

    fn publish(&self, event_id: i64, kind: &str, review: &Review) {
        self.bus.publish(Notice {
            event_id,
            kind: kind.to_string(),
            review_id: Some(review.id.clone()),
            review: Some(review.to_json(false)),
            keys: None,
        });
    }

    /// Envelope checks that run before the plugin's schema, all at once.
    fn envelope_violations(&self, attrs: &Map<String, Value>) -> Result<(), Error> {
        let mut violations = Vec::new();
        for key in ["plugin", "title"] {
            match attrs.get(key) {
                Some(Value::String(s)) if !s.is_empty() => {}
                _ => violations.push(Violation::new(format!("/{key}"), "is required")),
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
        for key in ["origin", "payload", "summary"] {
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
            Ok(())
        } else {
            Err(Error::Invalid(violations))
        }
    }
}
