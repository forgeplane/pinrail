//! The messages the person sends to an agent from its session's thread:
//! queued until the agent reports a status, which hands them over once.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use super::Db;
use super::events::insert_event;
use super::sessions::activity;
use crate::reviews::parse_datetime;

/// A message to the agent, and when the agent received it.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageRow {
    pub id: i64,
    pub text: String,
    pub sent_by: String,
    pub sent_at: DateTime<Utc>,
    /// when a status handed it to the agent; `None` while it is queued
    pub delivered_at: Option<DateTime<Utc>>,
}

impl MessageRow {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "text": self.text,
            "sent_by": self.sent_by,
            "sent_at": crate::reviews::iso(self.sent_at),
            "delivered_at": self.delivered_at.map(crate::reviews::iso),
        })
    }
}

/// The session and the message, as a notice about the message carries them.
pub fn message_notice(agent: &str, id: &str, message: &MessageRow) -> Value {
    json!({ "agent": agent, "id": id, "message": message.to_json() })
}

/// What deleting a message came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Deleted {
    /// deleted, with the event that announces it
    Yes(i64, MessageRow),
    /// the agent had received it already
    Delivered,
    /// no such message in the session
    Missing,
}

const COLUMNS: &str = "id, text, sent_by, sent_at, delivered_at";

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<MessageRow> {
    let time = |s: String| parse_datetime(&s).unwrap_or_default();
    Ok(MessageRow {
        id: row.get(0)?,
        text: row.get(1)?,
        sent_by: row.get(2)?,
        sent_at: time(row.get(3)?),
        delivered_at: row.get::<_, Option<String>>(4)?.map(time),
    })
}

/// Hands the session's queued messages to the agent at `at`, oldest first:
/// each is delivered once, as the status that delivers it is recorded.
pub(super) fn deliver(
    conn: &Connection,
    agent: &str,
    id: &str,
    at: DateTime<Utc>,
) -> rusqlite::Result<Vec<MessageRow>> {
    let mut stmt = conn.prepare(&format!(
        "UPDATE session_messages SET delivered_at = ?3
         WHERE agent = ?1 AND session = ?2 AND delivered_at IS NULL
         RETURNING {COLUMNS}"
    ))?;
    let mut messages = stmt
        .query_map(params![agent, id, crate::reviews::iso(at)], row_to_message)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    messages.sort_by_key(|m| m.id);
    Ok(messages)
}

impl Db {
    /// Queues a message to the agent, as the session's activity, with the
    /// event that announces it; `None` for a session that does not exist.
    pub fn add_message(
        &self,
        agent: &str,
        id: &str,
        text: &str,
        sent_by: &str,
    ) -> rusqlite::Result<Option<(MessageRow, i64)>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let now = Utc::now();
        let changed = tx.execute(
            "UPDATE sessions SET last_at = MAX(last_at, ?3) WHERE agent = ?1 AND id = ?2",
            params![agent, id, activity(now)],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        tx.execute(
            "INSERT INTO session_messages (agent, session, text, sent_by, sent_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![agent, id, text, sent_by, crate::reviews::iso(now)],
        )?;
        let message = MessageRow {
            id: tx.last_insert_rowid(),
            text: text.to_string(),
            sent_by: sent_by.to_string(),
            sent_at: parse_datetime(&crate::reviews::iso(now)).unwrap_or(now),
            delivered_at: None,
        };
        let event_id = insert_event(
            &tx,
            None,
            crate::events::SESSION_MESSAGE_SENT,
            None,
            &message_notice(agent, id, &message),
        )?;
        tx.commit()?;
        Ok(Some((message, event_id)))
    }

    /// Deletes a message the agent has not received yet.
    pub fn delete_message(&self, agent: &str, id: &str, message: i64) -> rusqlite::Result<Deleted> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let deleted = tx
            .query_row(
                &format!(
                    "DELETE FROM session_messages
                     WHERE agent = ?1 AND session = ?2 AND id = ?3 AND delivered_at IS NULL
                     RETURNING {COLUMNS}"
                ),
                params![agent, id, message],
                row_to_message,
            )
            .optional()?;
        let Some(deleted) = deleted else {
            let delivered = tx
                .query_row(
                    "SELECT 1 FROM session_messages WHERE agent = ?1 AND session = ?2 AND id = ?3",
                    params![agent, id, message],
                    |_| Ok(()),
                )
                .optional()?;
            return Ok(if delivered.is_some() {
                Deleted::Delivered
            } else {
                Deleted::Missing
            });
        };
        let event_id = insert_event(
            &tx,
            None,
            crate::events::SESSION_MESSAGE_DELETED,
            None,
            &message_notice(agent, id, &deleted),
        )?;
        tx.commit()?;
        Ok(Deleted::Yes(event_id, deleted))
    }

    /// The session's messages, oldest first.
    pub fn messages(&self, agent: &str, id: &str) -> rusqlite::Result<Vec<MessageRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM session_messages
             WHERE agent = ?1 AND session = ?2 ORDER BY id"
        ))?;
        stmt.query_map(params![agent, id], row_to_message)?
            .collect()
    }
}
