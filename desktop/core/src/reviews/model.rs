//! One review: the envelope, the payload the requester submitted and, once
//! decided, the decision. Status is derived, never stored.

use chrono::{DateTime, SecondsFormat, SubsecRound, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// The keys kept from a submitted `origin`; everything else is dropped.
pub const ORIGIN_KEYS: [&str; 5] = ["repo", "workflow", "run_id", "ref", "url"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Decided,
    Withdrawn,
    Expired,
    /// the person said no, and stop: nothing decided, the agent told
    Discarded,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Decided => "decided",
            Status::Withdrawn => "withdrawn",
            Status::Expired => "expired",
            Status::Discarded => "discarded",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Status::Pending),
            "decided" => Some(Status::Decided),
            "withdrawn" => Some(Status::Withdrawn),
            "expired" => Some(Status::Expired),
            "discarded" => Some(Status::Discarded),
            _ => None,
        }
    }
}

/// What a review is: a plugin's review, or questions an agent asks with
/// `pinrail ask`, which have no plugin and the app's own form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Review,
    Question,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Review => "review",
            Kind::Question => "question",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "review" => Some(Kind::Review),
            "question" => Some(Kind::Question),
            _ => None,
        }
    }
}

/// Where a decision or a discard was made: in the app, with the `pinrail`
/// CLI, or by another client of the API. It is what the client says, and
/// the API has no authentication, so it is not proof; it makes an honest
/// `pinrail decide` visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    App,
    Cli,
    Api,
}

impl Via {
    pub fn as_str(self) -> &'static str {
        match self {
            Via::App => "app",
            Via::Cli => "cli",
            Via::Api => "api",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "app" => Some(Via::App),
            "cli" => Some(Via::Cli),
            "api" => Some(Via::Api),
            _ => None,
        }
    }
}

/// Who sent a decision or a discard: where from, and the agent's session
/// the CLI ran in, if it ran in one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub via: Via,
    pub session: Option<String>,
    /// Who the client says made it, such as the agent the CLI runs under,
    /// or the name given to `pinrail discard --by`. The app's own requests
    /// name nobody: the person at the app is the server's user.
    pub by: Option<String>,
}

impl Source {
    /// A client of the API that says nothing about itself.
    pub fn api() -> Self {
        Source {
            via: Via::Api,
            session: None,
            by: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Decision {
    /// The person, for a decision made in the app; for one made elsewhere,
    /// whoever the client named, or nobody.
    pub decided_by: Option<String>,
    pub decided_at: DateTime<Utc>,
    pub data: Value,
    /// What was decided, summed up as the plugin declares; none when it
    /// declares no outcome summary or there is nothing to show.
    pub summary: Option<Value>,
    /// Where it was made; none for a decision recorded before this was kept.
    #[serde(default)]
    pub via: Option<Via>,
    /// The agent's session that made it with the CLI, if any.
    #[serde(default)]
    pub session: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Review {
    pub id: String,
    pub kind: Kind,
    /// the plugin's name, such as `list`; empty for questions
    pub plugin: String,
    /// the exact version it was submitted to, `1.2.3`
    pub plugin_version: String,
    /// the bundle it was submitted to, which it renders and validates with;
    /// none only for a review whose bundle could not be recorded
    pub plugin_bundle: Option<String>,
    pub title: String,
    pub origin: Map<String, Value>,
    pub requested_by: Option<String>,
    /// the agent's session it was asked from, which the CLI sets
    pub session: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revises: Option<String>,
    pub summary: Option<Value>,
    /// `None` in listings, which carry no payload.
    pub payload: Option<Value>,
    pub decision: Option<Decision>,
    pub agent_note: Option<String>,
    pub withdrawn_at: Option<DateTime<Utc>>,
    pub withdrawn_reason: Option<String>,
    /// Who withdrew the review, where, and from which agent session, as
    /// for a discard. A round withdrawn for a newer one is recorded as
    /// withdrawn by the newer round's requester, with no `via`.
    pub withdrawn_by: Option<String>,
    pub withdrawn_via: Option<Via>,
    pub withdrawn_session: Option<String>,
    pub discarded_at: Option<DateTime<Utc>>,
    pub discarded_by: Option<String>,
    pub discarded_reason: Option<String>,
    /// Where the discard was made, and the agent's session that made it
    /// with the CLI, as for a decision.
    pub discarded_via: Option<Via>,
    pub discarded_session: Option<String>,
    /// The files the review carries, by the names its payload uses. Read
    /// with a single review; listings leave them out, as they do the payload.
    pub attachments: Vec<crate::attachments::ReviewAttachment>,
    /// How many files the review carries and their bytes: in listings too,
    /// so a row can say so without the list.
    pub attachments_total: (u64, u64),
}

impl Review {
    pub fn status(&self, now: DateTime<Utc>) -> Status {
        if self.decision.is_some() {
            Status::Decided
        } else if self.withdrawn_at.is_some() {
            Status::Withdrawn
        } else if self.discarded_at.is_some() {
            Status::Discarded
        } else if self.expires_at.is_some_and(|at| at <= now) {
            Status::Expired
        } else {
            Status::Pending
        }
    }

    pub fn is_pending(&self, now: DateTime<Utc>) -> bool {
        self.status(now) == Status::Pending
    }

    /// The public representation. Listings pass `with_payload: false`.
    pub fn to_json(&self, with_payload: bool) -> Value {
        let mut map = json!({
            "id": self.id,
            "kind": self.kind.as_str(),
            "plugin": self.plugin,
            "plugin_version": self.plugin_version,
            "plugin_bundle": self.plugin_bundle,
            "title": self.title,
            "origin": self.origin,
            "requested_by": self.requested_by,
            "session": self.session,
            "created_at": iso(self.created_at),
            "expires_at": self.expires_at.map(iso),
            "revises": self.revises,
            "summary": self.summary,
            "status": self.status(Utc::now()).as_str(),
            "decision": self.decision.as_ref().map(|d| json!({
                "decided_by": d.decided_by,
                "decided_at": iso(d.decided_at),
                "data": d.data,
                "summary": d.summary,
                "via": d.via.map(Via::as_str),
                "session": d.session,
            })),
            "agent_note": self.agent_note,
            "withdrawn_at": self.withdrawn_at.map(iso),
            "withdrawn_reason": self.withdrawn_reason,
            "withdrawn_by": self.withdrawn_by,
            "withdrawn_via": self.withdrawn_via.map(Via::as_str),
            "withdrawn_session": self.withdrawn_session,
            "discarded_at": self.discarded_at.map(iso),
            "discarded_by": self.discarded_by,
            "discarded_reason": self.discarded_reason,
            "discarded_via": self.discarded_via.map(Via::as_str),
            "discarded_session": self.discarded_session,
            "attachments_total": { "count": self.attachments_total.0, "bytes": self.attachments_total.1 },
        });
        if with_payload {
            map["payload"] = self.payload.clone().unwrap_or(Value::Null);
            // what the review carries, never the bytes: those come from
            // GET /api/v1/reviews/{id}/attachments/{name}
            map["attachments"] = self
                .attachments
                .iter()
                .map(|a| {
                    json!({
                        "name": a.name,
                        "size": a.size,
                        "media_type": a.media_type,
                        "sha256": a.sha256,
                    })
                })
                .collect();
        }
        map
    }

    /// The `origin` map with only the known keys, every value a string.
    pub fn normalize_origin(origin: Option<&Value>) -> Map<String, Value> {
        let mut out = Map::new();
        if let Some(Value::Object(map)) = origin {
            for key in ORIGIN_KEYS {
                match map.get(key) {
                    None => {}
                    Some(Value::Null) => {
                        out.insert(key.to_string(), Value::Null);
                    }
                    Some(Value::String(s)) => {
                        out.insert(key.to_string(), Value::String(s.clone()));
                    }
                    Some(other) => {
                        out.insert(key.to_string(), Value::String(other.to_string()));
                    }
                }
            }
        }
        out
    }
}

/// ISO 8601 at second precision, `Z` suffixed, as every timestamp in the API is written.
pub fn iso(dt: DateTime<Utc>) -> String {
    dt.trunc_subsecs(0)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review() -> Review {
        Review {
            attachments: Vec::new(),
            attachments_total: (0, 0),
            id: "r_1".into(),
            kind: Kind::Review,
            plugin: "list".into(),
            plugin_version: "1.0.0".into(),
            plugin_bundle: None,
            title: "t".into(),
            origin: Map::new(),
            requested_by: None,
            session: None,
            created_at: parse_datetime("2026-09-11T10:00:00Z").unwrap(),
            expires_at: None,
            revises: None,
            summary: None,
            payload: Some(json!({})),
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
        }
    }

    #[test]
    fn status_is_derived() {
        let now = parse_datetime("2026-09-11T12:00:00Z").unwrap();
        let mut r = review();
        assert_eq!(r.status(now), Status::Pending);
        r.expires_at = Some(parse_datetime("2026-09-11T11:00:00Z").unwrap());
        assert_eq!(r.status(now), Status::Expired);
        r.expires_at = Some(parse_datetime("2026-09-11T13:00:00Z").unwrap());
        assert_eq!(r.status(now), Status::Pending);
        r.discarded_at = Some(now);
        assert_eq!(r.status(now), Status::Discarded);
        r.withdrawn_at = Some(now);
        assert_eq!(r.status(now), Status::Withdrawn);
        r.decision = Some(Decision {
            decided_by: Some("a".into()),
            decided_at: now,
            data: json!({}),
            summary: None,
            via: None,
            session: None,
        });
        assert_eq!(r.status(now), Status::Decided);
    }

    #[test]
    fn origin_keeps_known_keys_as_strings() {
        let origin = Review::normalize_origin(Some(&json!({
            "repo": "acme", "ref": 42, "junk": true, "url": null
        })));
        assert_eq!(origin.get("repo"), Some(&json!("acme")));
        assert_eq!(origin.get("ref"), Some(&json!("42")));
        assert_eq!(origin.get("url"), Some(&Value::Null));
        assert!(!origin.contains_key("junk"));
        assert!(Review::normalize_origin(Some(&json!("x"))).is_empty());
    }

    #[test]
    fn listings_drop_the_payload() {
        let r = review();
        assert!(r.to_json(true).get("payload").is_some());
        assert!(r.to_json(false).get("payload").is_none());
        assert_eq!(r.to_json(false)["created_at"], "2026-09-11T10:00:00Z");
    }
}
