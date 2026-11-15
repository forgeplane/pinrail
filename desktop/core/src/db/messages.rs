//! The messages the person sends to an agent from its session's thread:
//! queued until the agent reports a status, which claims them, and
//! delivered once the CLI acknowledges that it printed them. A claim that
//! is not acknowledged within a minute, as when the CLI was killed, lapses,
//! and the next status hands the message over again.

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
    /// when the agent received it; `None` until then
    pub delivered_at: Option<DateTime<Utc>>,
    /// `queued`; `claimed` while a status hands it over; `typing` once
    /// typed into the agent's terminal, until its prompt hook confirms
    /// it; `unconfirmed` when it did not; `typed` for an agent with no
    /// prompt hook; or `delivered`
    pub state: String,
    /// `type`, into the terminal of the run Pinrail runs it in, or `queue`,
    /// for its next status; `None` for a message from before routes
    pub route: Option<String>,
    /// why a message to be typed waits: `working`, `needs_input`,
    /// `used_since_idle` or `not_foreground`
    pub held: Option<String>,
}

/// How long a status's claim on a message lasts without the CLI's
/// acknowledgement.
pub const CLAIM_FOR: chrono::Duration = chrono::Duration::seconds(60);

impl MessageRow {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "text": self.text,
            "sent_by": self.sent_by,
            "sent_at": crate::reviews::iso(self.sent_at),
            "delivered_at": self.delivered_at.map(crate::reviews::iso),
            "state": self.state,
            "route": self.route,
            "held": self.held,
        })
    }
}

/// The session and the message, as a notice about the message carries them.
pub fn message_notice(agent: &str, id: &str, message: &MessageRow) -> Value {
    json!({ "agent": agent, "id": id, "message": message.to_json() })
}

/// A message named by its session: `(agent, session, message)`.
pub type MessageKey = (String, String, i64);

/// The state a message moves to, with what changes with it.
#[derive(Debug, Clone, Copy, Default)]
pub struct MessageChange<'a> {
    pub state: &'a str,
    /// why it waits; `None` clears it
    pub held: Option<&'a str>,
    /// a new route, or `None` to keep it
    pub route: Option<&'a str>,
    /// the run it is typed into
    pub run: Option<&'a str>,
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

const COLUMNS: &str = "id, text, sent_by, sent_at, delivered_at, state, route, held";

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<MessageRow> {
    let time = |s: String| parse_datetime(&s).unwrap_or_default();
    Ok(MessageRow {
        id: row.get(0)?,
        text: row.get(1)?,
        sent_by: row.get(2)?,
        sent_at: time(row.get(3)?),
        delivered_at: row.get::<_, Option<String>>(4)?.map(time),
        state: row.get(5)?,
        route: row.get(6)?,
        held: row.get(7)?,
    })
}

/// Claims the session's queued messages for the status reported at `at`,
/// oldest first, with those whose claim lapsed: the CLI prints them, then
/// acknowledges them.
pub(super) fn claim(
    conn: &Connection,
    agent: &str,
    id: &str,
    at: DateTime<Utc>,
) -> rusqlite::Result<Vec<MessageRow>> {
    let mut stmt = conn.prepare(&format!(
        "UPDATE session_messages SET state = 'claimed', claimed_at = ?3, attempts = attempts + 1
         WHERE agent = ?1 AND session = ?2
           AND (state = 'queued' OR (state = 'claimed' AND claimed_at < ?4))
         RETURNING {COLUMNS}"
    ))?;
    let mut messages = stmt
        .query_map(
            params![
                agent,
                id,
                crate::reviews::iso(at),
                crate::reviews::iso(at - CLAIM_FOR)
            ],
            row_to_message,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    messages.sort_by_key(|m| m.id);
    Ok(messages)
}

impl Db {
    /// Queues a message to the agent, as the session's activity, with the
    /// event that announces it; `None` for a session that does not exist.
    /// A message sent again with the `local_id` of one already queued is
    /// that message, without an event.
    pub fn add_message(
        &self,
        agent: &str,
        id: &str,
        text: &str,
        sent_by: &str,
        local_id: Option<&str>,
        route: &str,
    ) -> rusqlite::Result<Option<(MessageRow, Option<i64>)>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let now = Utc::now();
        if let Some(local_id) = local_id {
            let earlier = tx
                .query_row(
                    &format!(
                        "SELECT {COLUMNS} FROM session_messages
                         WHERE agent = ?1 AND session = ?2 AND local_id = ?3"
                    ),
                    params![agent, id, local_id],
                    row_to_message,
                )
                .optional()?;
            if let Some(earlier) = earlier {
                return Ok(Some((earlier, None)));
            }
        }
        let changed = tx.execute(
            // the person messaged it, so it shows from now on
            "UPDATE sessions SET last_at = MAX(last_at, ?3), hidden = 0 WHERE agent = ?1 AND id = ?2",
            params![agent, id, activity(now)],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        tx.execute(
            "INSERT INTO session_messages (agent, session, text, sent_by, sent_at, local_id, route)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                agent,
                id,
                text,
                sent_by,
                crate::reviews::iso(now),
                local_id,
                route
            ],
        )?;
        let message = MessageRow {
            id: tx.last_insert_rowid(),
            text: text.to_string(),
            sent_by: sent_by.to_string(),
            sent_at: parse_datetime(&crate::reviews::iso(now)).unwrap_or(now),
            delivered_at: None,
            state: "queued".to_string(),
            route: Some(route.to_string()),
            held: None,
        };
        let event_id = insert_event(
            &tx,
            None,
            crate::events::SESSION_MESSAGE_SENT,
            None,
            &message_notice(agent, id, &message),
        )?;
        tx.commit()?;
        Ok(Some((message, Some(event_id))))
    }

    /// Delivers the claimed messages of the session among `ids` at `at`,
    /// as the CLI acknowledges that it printed them, with the event that
    /// announces them; none when they were delivered already, or are not
    /// the session's.
    pub fn acknowledge_messages(
        &self,
        agent: &str,
        id: &str,
        ids: &[i64],
        at: DateTime<Utc>,
    ) -> rusqlite::Result<Option<(Vec<MessageRow>, i64)>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut delivered = Vec::new();
        for message in ids {
            let row = tx
                .query_row(
                    &format!(
                        "UPDATE session_messages SET state = 'delivered', delivered_at = ?4
                         WHERE agent = ?1 AND session = ?2 AND id = ?3 AND state = 'claimed'
                         RETURNING {COLUMNS}"
                    ),
                    params![agent, id, message, crate::reviews::iso(at)],
                    row_to_message,
                )
                .optional()?;
            delivered.extend(row);
        }
        if delivered.is_empty() {
            return Ok(None);
        }
        delivered.sort_by_key(|m| m.id);
        let event_id = insert_event(
            &tx,
            None,
            crate::events::SESSION_MESSAGES_DELIVERED,
            None,
            &super::sessions::delivered_notice(agent, id, &delivered),
        )?;
        tx.commit()?;
        Ok(Some((delivered, event_id)))
    }

    /// Deletes a message no status has handed to the agent yet.
    pub fn delete_message(&self, agent: &str, id: &str, message: i64) -> rusqlite::Result<Deleted> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let deleted = tx
            .query_row(
                &format!(
                    "DELETE FROM session_messages
                     WHERE agent = ?1 AND session = ?2 AND id = ?3 AND state = 'queued'
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

    /// The session's messages waiting to be typed into its run's terminal,
    /// oldest first.
    pub fn messages_to_type(&self, agent: &str, id: &str) -> rusqlite::Result<Vec<MessageRow>> {
        let conn = self.read();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM session_messages
             WHERE agent = ?1 AND session = ?2 AND route = 'type' AND state = 'queued'
             ORDER BY id"
        ))?;
        stmt.query_map(params![agent, id], row_to_message)?
            .collect()
    }

    /// One of the session's messages.
    pub fn message(
        &self,
        agent: &str,
        id: &str,
        message: i64,
    ) -> rusqlite::Result<Option<MessageRow>> {
        self.read()
            .query_row(
                &format!(
                    "SELECT {COLUMNS} FROM session_messages
                     WHERE agent = ?1 AND session = ?2 AND id = ?3"
                ),
                params![agent, id, message],
                row_to_message,
            )
            .optional()
    }

    /// Moves a message to `state` from one of `from`, with why it waits
    /// and, for `typing`, its run and the time it is typed, with the event
    /// that announces it; `None` when it was in none of `from`, as when a
    /// status claimed it meanwhile.
    pub fn move_message(
        &self,
        agent: &str,
        id: &str,
        message: i64,
        from: &[&str],
        to: MessageChange<'_>,
    ) -> rusqlite::Result<Option<(MessageRow, i64)>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let now = crate::reviews::iso(Utc::now());
        let from = from
            .iter()
            .map(|s| format!("'{s}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let moved = tx
            .query_row(
                &format!(
                    "UPDATE session_messages SET
                       state = ?4, held = ?5, route = COALESCE(?6, route),
                       run_id = COALESCE(?7, run_id),
                       typed_at = CASE WHEN ?4 = 'typing' THEN ?8 ELSE typed_at END,
                       attempts = attempts + CASE WHEN ?4 = 'typing' THEN 1 ELSE 0 END,
                       delivered_at = CASE WHEN ?4 = 'delivered' THEN ?8 ELSE delivered_at END
                     WHERE agent = ?1 AND session = ?2 AND id = ?3 AND state IN ({from})
                     RETURNING {COLUMNS}"
                ),
                params![agent, id, message, to.state, to.held, to.route, to.run, now],
                row_to_message,
            )
            .optional()?;
        let Some(moved) = moved else {
            return Ok(None);
        };
        let event_id = insert_event(
            &tx,
            None,
            crate::events::SESSION_MESSAGE_STATE,
            None,
            &message_notice(agent, id, &moved),
        )?;
        tx.commit()?;
        Ok(Some((moved, event_id)))
    }

    /// The messages typed into the session's run and not yet confirmed,
    /// oldest first, which its prompt hook may confirm.
    pub fn typed_messages(&self, agent: &str, id: &str) -> rusqlite::Result<Vec<MessageRow>> {
        let conn = self.read();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM session_messages
             WHERE agent = ?1 AND session = ?2 AND state IN ('typing', 'unconfirmed')
             ORDER BY id"
        ))?;
        stmt.query_map(params![agent, id], row_to_message)?
            .collect()
    }

    /// The messages typed before `before` that no prompt confirmed, and
    /// those to be typed that have waited since before `held_before`, by
    /// session as `(agent, session, message)`.
    pub fn overdue_messages(
        &self,
        before: DateTime<Utc>,
        held_before: DateTime<Utc>,
    ) -> rusqlite::Result<(Vec<MessageKey>, Vec<MessageKey>)> {
        let conn = self.read();
        let query =
            |sql: &str, at: DateTime<Utc>| -> rusqlite::Result<Vec<(String, String, i64)>> {
                conn.prepare(sql)?
                    .query_map(params![crate::reviews::iso(at)], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                    })?
                    .collect()
            };
        Ok((
            query(
                "SELECT agent, session, id FROM session_messages
                 WHERE state = 'typing' AND typed_at < ?1",
                before,
            )?,
            query(
                "SELECT agent, session, id FROM session_messages
                 WHERE route = 'type' AND state = 'queued' AND sent_at < ?1",
                held_before,
            )?,
        ))
    }

    /// The session's messages, oldest first.
    pub fn messages(&self, agent: &str, id: &str) -> rusqlite::Result<Vec<MessageRow>> {
        let conn = self.read();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM session_messages
             WHERE agent = ?1 AND session = ?2 ORDER BY id"
        ))?;
        stmt.query_map(params![agent, id], row_to_message)?
            .collect()
    }
}
