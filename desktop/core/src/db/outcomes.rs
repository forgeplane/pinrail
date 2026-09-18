//! How a review ends: a decision, a withdrawal, or a discard. Each is
//! written once, guarded by the primary key on `decisions`, and each
//! appends the event that announces it.

use chrono::{DateTime, Utc};
use rusqlite::params;
use serde_json::Value;

use super::Db;
use super::events::insert_event;
use crate::reviews::Decision;

impl Db {
    /// Records the decision; `None` when the review had already ended.
    pub fn insert_decision(
        &self,
        id: &str,
        decision: &Decision,
        agent_note: Option<&str>,
    ) -> rusqlite::Result<Option<i64>> {
        self.insert_outcome(
            id,
            Outcome {
                kind: "decided",
                at: decision.decided_at,
                by: Some(&decision.decided_by),
                reason: None,
                data: Some(&decision.data),
                agent_note,
            },
            crate::events::DECIDED,
            Some(&decision.decided_by),
            Value::Null,
        )
    }

    /// Records the requester's withdrawal; `None` when the review had
    /// already ended.
    pub fn insert_withdrawal(
        &self,
        id: &str,
        at: DateTime<Utc>,
        reason: Option<&str>,
    ) -> rusqlite::Result<Option<i64>> {
        let attrs = reason
            .map(|r| serde_json::json!({ "reason": r }))
            .unwrap_or(Value::Null);
        self.insert_outcome(
            id,
            Outcome {
                kind: "withdrawn",
                at,
                by: None,
                reason,
                data: None,
                agent_note: None,
            },
            crate::events::WITHDRAWN,
            None,
            attrs,
        )
    }

    /// Records the person's "no, and stop"; `None` when the review had
    /// already ended.
    pub fn insert_discard(
        &self,
        id: &str,
        at: DateTime<Utc>,
        by: &str,
        reason: Option<&str>,
    ) -> rusqlite::Result<Option<i64>> {
        let attrs = match reason {
            Some(r) => serde_json::json!({ "by": by, "reason": r }),
            None => serde_json::json!({ "by": by }),
        };
        self.insert_outcome(
            id,
            Outcome {
                kind: "discarded",
                at,
                by: Some(by),
                reason,
                data: None,
                agent_note: None,
            },
            crate::events::DISCARDED,
            Some(by),
            attrs,
        )
    }

    /// The one way a review ends. The primary key on `outcomes` admits one
    /// row per review whichever the kind, so the loser of a race, of any
    /// kind against any other, gets `None`; the event goes in the same
    /// transaction as the outcome.
    fn insert_outcome(
        &self,
        id: &str,
        outcome: Outcome<'_>,
        event: &str,
        actor: Option<&str>,
        attrs: Value,
    ) -> rusqlite::Result<Option<i64>> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO outcomes (review_id, kind, at, by, reason, data, agent_note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                outcome.kind,
                crate::reviews::iso(outcome.at),
                outcome.by,
                outcome.reason,
                outcome.data.map(Value::to_string),
                outcome.agent_note
            ],
        )?;
        if inserted == 0 {
            return Ok(None);
        }
        let event_id = insert_event(&tx, Some(id), event, actor, &attrs)?;
        tx.commit()?;
        Ok(Some(event_id))
    }
}

/// An outcome as written: which kind, when, and what each kind carries.
struct Outcome<'a> {
    kind: &'a str,
    at: DateTime<Utc>,
    by: Option<&'a str>,
    reason: Option<&'a str>,
    data: Option<&'a Value>,
    agent_note: Option<&'a str>,
}
