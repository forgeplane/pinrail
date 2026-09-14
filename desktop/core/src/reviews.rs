//! The reviews service: submit, read, list, decide, withdraw, discard,
//! wait, expire.
//!
//! Writes go to the database, then out on the bus. Status transitions are
//! `pending -> decided | withdrawn | discarded | expired` and nothing else;
//! the database enforces that with the primary keys on `decisions`,
//! `withdrawals` and `discards`, so two racing decisions cannot both win.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde_json::{Map, Value};

use crate::db::{Db, Event, Filters};
use crate::error::{Error, Violation};
use crate::events::{self, Bus, Notice};
use crate::plugins::Registry;
use crate::review::{Decision, Review, Status, parse_datetime};

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 500;

#[derive(Debug, Clone)]
pub struct Reviews {
    db: Arc<Db>,
    registry: Arc<Registry>,
    bus: Bus,
    user: String,
}

impl Reviews {
    pub fn new(db: Arc<Db>, registry: Arc<Registry>, bus: Bus, user: String) -> Self {
        Reviews {
            db,
            registry,
            bus,
            user,
        }
    }

    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    /// Submits a review from a request body. `plugin` and `title` are
    /// required; `origin`, `payload`, `summary`, `revises`, `expires_at` and
    /// `requested_by` are optional. Every failure is `invalid` with
    /// violations pointing into the body.
    pub fn submit(&self, body: &Value, actor: Option<&str>) -> Result<Review, Error> {
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
        self.registry.ensure_snapshot(&plugin)?;

        let review = Review {
            id: crate::id::next(),
            plugin: plugin.name.clone(),
            plugin_version: plugin.version,
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
        };
        let event_id = self.db.insert_review(&review, actor)?;
        self.publish(event_id, events::CREATED, &review);
        Ok(review)
    }

    pub fn get(&self, id: &str) -> Result<Review, Error> {
        self.db
            .get_review(id)?
            .ok_or_else(|| Error::NotFound(id.to_string()))
    }

    pub fn list(&self, filters: &Filters) -> Result<Vec<Review>, Error> {
        let mut filters = filters.clone();
        filters.limit = match filters.limit {
            0 => DEFAULT_LIMIT,
            n => n.min(MAX_LIMIT),
        };
        Ok(self.db.list(&filters, Utc::now())?)
    }

    pub fn pending_count(&self) -> Result<usize, Error> {
        let filters = Filters {
            statuses: vec![Status::Pending],
            limit: MAX_LIMIT,
            ..Filters::default()
        };
        Ok(self.db.list(&filters, Utc::now())?.len())
    }

    pub fn rounds(&self, id: &str) -> Result<Vec<Review>, Error> {
        let rounds = self.db.rounds(id)?;
        if rounds.is_empty() {
            return Err(Error::NotFound(id.to_string()));
        }
        Ok(rounds)
    }

    pub fn events(&self, id: &str) -> Result<Vec<Event>, Error> {
        if !self.db.exists(id)? {
            return Err(Error::NotFound(id.to_string()));
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

    pub fn publish_plain(&self, event_id: i64, kind: &str) {
        self.bus.publish(Notice {
            event_id,
            kind: kind.to_string(),
            review_id: None,
            review: None,
            keys: None,
        });
    }

    /// A notice about settings: which of them changed.
    pub fn publish_keys(&self, event_id: i64, kind: &str, keys: Vec<String>) {
        self.bus.publish(Notice {
            event_id,
            kind: kind.to_string(),
            review_id: None,
            review: None,
            keys: Some(keys),
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
            Some(Value::String(s)) if parse_datetime(s).is_some() => {}
            Some(_) => violations.push(Violation::new(
                "/expires_at",
                "must be an ISO 8601 datetime",
            )),
        }
        match attrs.get("revises") {
            None | Some(Value::Null) => {}
            Some(Value::String(id)) => {
                if !self.db.exists(id)? {
                    violations.push(Violation::new("/revises", format!("unknown review {id}")));
                }
            }
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
        if violations.is_empty() {
            Ok(())
        } else {
            Err(Error::Invalid(violations))
        }
    }
}
