//! SQLite: the record of truth. Reviews, decisions and withdrawals are
//! written once; events are appended; the one-decision rule is the primary
//! key on `decisions`.

use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value};

use crate::review::{Decision, Review, Status, parse_datetime};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS reviews (
  id             TEXT PRIMARY KEY,
  plugin         TEXT NOT NULL,
  plugin_version INTEGER NOT NULL,
  title          TEXT NOT NULL,
  origin         TEXT NOT NULL,
  requested_by   TEXT,
  payload        TEXT NOT NULL,
  summary        TEXT,
  revises        TEXT REFERENCES reviews(id),
  expires_at     TEXT,
  created_at     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS reviews_created ON reviews(created_at DESC);
CREATE INDEX IF NOT EXISTS reviews_revises ON reviews(revises);

CREATE TABLE IF NOT EXISTS decisions (
  review_id   TEXT PRIMARY KEY REFERENCES reviews(id),
  decided_by  TEXT NOT NULL,
  decided_at  TEXT NOT NULL,
  data        TEXT NOT NULL,
  agent_note  TEXT
);

CREATE TABLE IF NOT EXISTS withdrawals (
  review_id    TEXT PRIMARY KEY REFERENCES reviews(id),
  withdrawn_at TEXT NOT NULL,
  reason       TEXT
);

CREATE TABLE IF NOT EXISTS events (
  id        INTEGER PRIMARY KEY,
  review_id TEXT REFERENCES reviews(id),
  kind      TEXT NOT NULL,
  actor     TEXT,
  at        TEXT NOT NULL,
  attrs     TEXT
);
CREATE INDEX IF NOT EXISTS events_review ON events(review_id, id);

CREATE TABLE IF NOT EXISTS plugin_dirs (path TEXT PRIMARY KEY, added_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
"#;

#[derive(Debug, Default, Clone)]
pub struct Filters {
    pub statuses: Vec<Status>,
    pub plugin: Option<String>,
    pub repo: Option<String>,
    pub workflow: Option<String>,
    pub reference: Option<String>,
    pub run_id: Option<String>,
    pub text: Option<String>,
    pub include_revised: bool,
    /// Only reviews with an id below this one, for paging newest first.
    pub cursor: Option<String>,
    pub limit: usize,
}

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
            "at": crate::review::iso(self.at),
            "attrs": self.attrs,
        })
    }
}

#[derive(Debug)]
pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Db> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;",
        )?;
        Self::init(conn)
    }

    pub fn in_memory() -> rusqlite::Result<Db> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> rusqlite::Result<Db> {
        conn.execute_batch(SCHEMA)?;
        Ok(Db {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert_review(&self, review: &Review, actor: Option<&str>) -> rusqlite::Result<i64> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO reviews (id, plugin, plugin_version, title, origin, requested_by, payload, summary, revises, expires_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                review.id,
                review.plugin,
                review.plugin_version,
                review.title,
                Value::Object(review.origin.clone()).to_string(),
                review.requested_by,
                review.payload.clone().unwrap_or(Value::Object(Map::new())).to_string(),
                review.summary.as_ref().map(|s| s.to_string()),
                review.revises,
                review.expires_at.map(crate::review::iso),
                crate::review::iso(review.created_at),
            ],
        )?;
        let event_id = insert_event(
            &tx,
            Some(&review.id),
            crate::events::CREATED,
            actor,
            &Value::Null,
        )?;
        tx.commit()?;
        Ok(event_id)
    }

    pub fn get_review(&self, id: &str) -> rusqlite::Result<Option<Review>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(&format!("{SELECT} WHERE r.id = ?1"), params![id], |row| {
            row_to_review(row, true)
        })
        .optional()
    }

    pub fn exists(&self, id: &str) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT 1 FROM reviews WHERE id = ?1", params![id], |_| {
            Ok(())
        })
        .optional()
        .map(|r| r.is_some())
    }

    /// Reviews matching the filters, newest first, without payloads. Status
    /// filtering happens after the query because status is derived.
    pub fn list(&self, filters: &Filters, now: DateTime<Utc>) -> rusqlite::Result<Vec<Review>> {
        let mut sql = format!("{SELECT} WHERE 1=1");
        let mut args: Vec<String> = Vec::new();
        let mut push = |value: &str| {
            args.push(value.to_string());
            args.len()
        };
        let exact = [
            ("r.plugin", &filters.plugin),
            ("json_extract(r.origin, '$.repo')", &filters.repo),
            ("json_extract(r.origin, '$.workflow')", &filters.workflow),
            ("json_extract(r.origin, '$.ref')", &filters.reference),
            ("json_extract(r.origin, '$.run_id')", &filters.run_id),
        ];
        for (column, value) in exact {
            if let Some(v) = value {
                let n = push(v);
                sql.push_str(&format!(" AND {column} = ?{n}"));
            }
        }
        if let Some(v) = &filters.text {
            let n = push(&format!("%{}%", v.replace('%', "\\%")));
            sql.push_str(&format!(
                " AND (r.title LIKE ?{n} ESCAPE '\\' OR r.payload LIKE ?{n} ESCAPE '\\')"
            ));
        }
        if let Some(c) = &filters.cursor {
            let n = push(c);
            sql.push_str(&format!(" AND r.id < ?{n}"));
        }
        if !filters.include_revised {
            sql.push_str(" AND NOT EXISTS (SELECT 1 FROM reviews n WHERE n.revises = r.id)");
        }
        sql.push_str(" ORDER BY r.id DESC");

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            args.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(params.as_slice(), |row| row_to_review(row, false))?;
        let mut out = Vec::new();
        for review in rows {
            let review = review?;
            if !filters.statuses.is_empty() && !filters.statuses.contains(&review.status(now)) {
                continue;
            }
            out.push(review);
            if filters.limit > 0 && out.len() >= filters.limit {
                break;
            }
        }
        Ok(out)
    }

    /// Every round of a review's chain, oldest first, without payloads.
    pub fn rounds(&self, id: &str) -> rusqlite::Result<Vec<Review>> {
        let conn = self.conn.lock().unwrap();
        let mut root = id.to_string();
        loop {
            let prev: Option<Option<String>> = conn
                .query_row(
                    "SELECT revises FROM reviews WHERE id = ?1",
                    params![root],
                    |row| row.get(0),
                )
                .optional()?;
            match prev {
                Some(Some(p)) => root = p,
                Some(None) => break,
                None => return Ok(Vec::new()),
            }
        }
        let mut out = Vec::new();
        let mut frontier = vec![root];
        while let Some(current) = frontier.pop() {
            let review = conn.query_row(
                &format!("{SELECT} WHERE r.id = ?1"),
                params![current],
                |row| row_to_review(row, false),
            )?;
            out.push(review);
            let mut stmt = conn.prepare("SELECT id FROM reviews WHERE revises = ?1 ORDER BY id")?;
            let next: Vec<String> = stmt
                .query_map(params![current], |row| row.get(0))?
                .collect::<Result<_, _>>()?;
            for n in next.into_iter().rev() {
                frontier.push(n);
            }
        }
        Ok(out)
    }

    /// Records the decision; `false` when one already exists.
    pub fn insert_decision(
        &self,
        id: &str,
        decision: &Decision,
        agent_note: Option<&str>,
    ) -> rusqlite::Result<Option<i64>> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO decisions (review_id, decided_by, decided_at, data, agent_note) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id,
                decision.decided_by,
                crate::review::iso(decision.decided_at),
                decision.data.to_string(),
                agent_note
            ],
        )?;
        if inserted == 0 {
            return Ok(None);
        }
        let event_id = insert_event(
            &tx,
            Some(id),
            crate::events::DECIDED,
            Some(&decision.decided_by),
            &Value::Null,
        )?;
        tx.commit()?;
        Ok(Some(event_id))
    }

    pub fn insert_withdrawal(
        &self,
        id: &str,
        at: DateTime<Utc>,
        reason: Option<&str>,
    ) -> rusqlite::Result<Option<i64>> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO withdrawals (review_id, withdrawn_at, reason) VALUES (?1, ?2, ?3)",
            params![id, crate::review::iso(at), reason],
        )?;
        if inserted == 0 {
            return Ok(None);
        }
        let attrs = reason
            .map(|r| serde_json::json!({ "reason": r }))
            .unwrap_or(Value::Null);
        let event_id = insert_event(&tx, Some(id), crate::events::WITHDRAWN, None, &attrs)?;
        tx.commit()?;
        Ok(Some(event_id))
    }

    pub fn append_event(
        &self,
        review_id: Option<&str>,
        kind: &str,
        actor: Option<&str>,
        attrs: &Value,
    ) -> rusqlite::Result<i64> {
        let conn = self.conn.lock().unwrap();
        insert_event(&conn, review_id, kind, actor, attrs)
    }

    pub fn has_event(&self, review_id: &str, kind: &str) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT 1 FROM events WHERE review_id = ?1 AND kind = ?2 LIMIT 1",
            params![review_id, kind],
            |_| Ok(()),
        )
        .optional()
        .map(|r| r.is_some())
    }

    pub fn events_for(&self, review_id: &str) -> rusqlite::Result<Vec<Event>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, review_id, kind, actor, at, attrs FROM events WHERE review_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![review_id], row_to_event)?;
        rows.collect()
    }

    pub fn events_after(&self, after: i64, limit: usize) -> rusqlite::Result<Vec<Event>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, review_id, kind, actor, at, attrs FROM events WHERE id > ?1 ORDER BY id LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![after, limit as i64], row_to_event)?;
        rows.collect()
    }

    /// Reviews whose expiry has passed with no decision or withdrawal and no
    /// `expired` event yet.
    pub fn newly_expired(&self, now: DateTime<Utc>) -> rusqlite::Result<Vec<Review>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "{SELECT} WHERE r.expires_at IS NOT NULL AND r.expires_at <= ?1
               AND d.review_id IS NULL AND w.review_id IS NULL
               AND NOT EXISTS (SELECT 1 FROM events e WHERE e.review_id = r.id AND e.kind = 'expired')
             ORDER BY r.id"
        ))?;
        let rows = stmt.query_map(params![crate::review::iso(now)], |row| {
            row_to_review(row, false)
        })?;
        rows.collect()
    }

    pub fn plugin_dirs(&self) -> rusqlite::Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT path FROM plugin_dirs ORDER BY added_at, path")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect()
    }

    pub fn add_plugin_dir(&self, path: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO plugin_dirs (path, added_at) VALUES (?1, ?2)",
            params![path, crate::review::iso(Utc::now())],
        )?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

const SELECT: &str = "SELECT r.id, r.plugin, r.plugin_version, r.title, r.origin, r.requested_by, r.payload, r.summary,
    r.revises, r.expires_at, r.created_at,
    d.decided_by, d.decided_at, d.data, d.agent_note,
    w.withdrawn_at, w.reason
  FROM reviews r
  LEFT JOIN decisions d ON d.review_id = r.id
  LEFT JOIN withdrawals w ON w.review_id = r.id";

fn insert_event(
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
            crate::review::iso(Utc::now()),
            if attrs.is_null() {
                None
            } else {
                Some(attrs.to_string())
            }
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn row_to_review(row: &rusqlite::Row<'_>, with_payload: bool) -> rusqlite::Result<Review> {
    let origin: String = row.get(4)?;
    let payload: String = row.get(6)?;
    let summary: Option<String> = row.get(7)?;
    let expires_at: Option<String> = row.get(9)?;
    let created_at: String = row.get(10)?;
    let decided_by: Option<String> = row.get(11)?;
    let decided_at: Option<String> = row.get(12)?;
    let data: Option<String> = row.get(13)?;
    let withdrawn_at: Option<String> = row.get(15)?;
    let decision = match (decided_by, decided_at, data) {
        (Some(decided_by), Some(at), Some(data)) => Some(Decision {
            decided_by,
            decided_at: parse_datetime(&at).unwrap_or_default(),
            data: serde_json::from_str(&data).unwrap_or(Value::Null),
        }),
        _ => None,
    };
    Ok(Review {
        id: row.get(0)?,
        plugin: row.get(1)?,
        plugin_version: row.get::<_, i64>(2)? as u32,
        title: row.get(3)?,
        origin: serde_json::from_str::<Value>(&origin)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default(),
        requested_by: row.get(5)?,
        payload: if with_payload {
            Some(serde_json::from_str(&payload).unwrap_or(Value::Null))
        } else {
            None
        },
        summary: summary.and_then(|s| serde_json::from_str(&s).ok()),
        revises: row.get(8)?,
        expires_at: expires_at.and_then(|s| parse_datetime(&s)),
        created_at: parse_datetime(&created_at).unwrap_or_default(),
        decision,
        agent_note: row.get(14)?,
        withdrawn_at: withdrawn_at.and_then(|s| parse_datetime(&s)),
        withdrawn_reason: row.get(16)?,
    })
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
