//! The append-only record of what happened: every notice the app broadcasts
//! is a row here first, so a client that was away can catch up.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde_json::Value;

use super::Db;
use crate::reviews::parse_datetime;

#[derive(Debug, Clone)]
pub struct Event {
    pub id: i64,
    pub review_id: Option<String>,
    pub kind: String,
    pub actor: Option<String>,
    pub at: DateTime<Utc>,
    pub attrs: Value,
}

impl Event {
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "id": self.id,
            "review_id": self.review_id,
            "kind": self.kind,
            "actor": self.actor,
            "at": crate::reviews::iso(self.at),
            "attrs": self.attrs,
        })
    }
}

impl Db {
    pub fn append_event(
        &self,
        review_id: Option<&str>,
        kind: &str,
        actor: Option<&str>,
        attrs: &Value,
    ) -> rusqlite::Result<i64> {
        let conn = self.conn();
        insert_event(&conn, review_id, kind, actor, attrs)
    }

    pub fn events_for(&self, review_id: &str) -> rusqlite::Result<Vec<Event>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, review_id, kind, actor, at, attrs FROM events WHERE review_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![review_id], row_to_event)?;
        rows.collect()
    }

    pub fn events_after(&self, after: i64, limit: usize) -> rusqlite::Result<Vec<Event>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, review_id, kind, actor, at, attrs FROM events WHERE id > ?1 ORDER BY id LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![after, limit as i64], row_to_event)?;
        rows.collect()
    }
}

pub(super) fn insert_event(
    conn: &Connection,
    review_id: Option<&str>,
    kind: &str,
    actor: Option<&str>,
    attrs: &Value,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO events (review_id, kind, actor, at, attrs) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            review_id,
            kind,
            actor,
            crate::reviews::iso(Utc::now()),
            if attrs.is_null() {
                None
            } else {
                Some(attrs.to_string())
            }
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    let at: String = row.get(4)?;
    let attrs: Option<String> = row.get(5)?;
    Ok(Event {
        id: row.get(0)?,
        review_id: row.get(1)?,
        kind: row.get(2)?,
        actor: row.get(3)?,
        at: parse_datetime(&at).unwrap_or_default(),
        attrs: attrs
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null),
    })
}
