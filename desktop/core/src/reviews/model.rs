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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Decision {
    pub decided_by: String,
    pub decided_at: DateTime<Utc>,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Review {
    pub id: String,
    pub plugin: String,
    /// the major line the review renders from
    pub plugin_version: u32,
    /// the exact version it was submitted under, `1.2.3`
    pub plugin_release: String,
    pub title: String,
    pub origin: Map<String, Value>,
    pub requested_by: Option<String>,
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
    pub discarded_at: Option<DateTime<Utc>>,
    pub discarded_by: Option<String>,
    pub discarded_reason: Option<String>,
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
            "plugin": self.plugin,
            "plugin_version": self.plugin_version,
            "plugin_release": self.plugin_release,
            "title": self.title,
            "origin": self.origin,
            "requested_by": self.requested_by,
            "created_at": iso(self.created_at),
            "expires_at": self.expires_at.map(iso),
            "revises": self.revises,
            "summary": self.summary,
            "status": self.status(Utc::now()).as_str(),
            "decision": self.decision.as_ref().map(|d| json!({
                "decided_by": d.decided_by,
                "decided_at": iso(d.decided_at),
                "data": d.data,
            })),
            "agent_note": self.agent_note,
            "withdrawn_at": self.withdrawn_at.map(iso),
            "withdrawn_reason": self.withdrawn_reason,
            "discarded_at": self.discarded_at.map(iso),
            "discarded_by": self.discarded_by,
            "discarded_reason": self.discarded_reason,
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
            plugin: "list".into(),
            plugin_version: 1,
            plugin_release: "1.0.0".into(),
            title: "t".into(),
            origin: Map::new(),
            requested_by: None,
            created_at: parse_datetime("2026-09-11T10:00:00Z").unwrap(),
            expires_at: None,
            revises: None,
            summary: None,
            payload: Some(json!({})),
            decision: None,
            agent_note: None,
            withdrawn_at: None,
            withdrawn_reason: None,
            discarded_at: None,
            discarded_by: None,
            discarded_reason: None,
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
            decided_by: "a".into(),
            decided_at: now,
            data: json!({}),
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
