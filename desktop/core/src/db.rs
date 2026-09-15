//! SQLite: the record of truth. Reviews and their outcomes are
//! written once; events are appended; the one-decision rule is the primary
//! key on `decisions`.

use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value};

use crate::review::{Decision, Review, Status, parse_datetime};

/// The tables as they were before versioning: step 0, run once for a new
/// file and never edited again. A later step creates the tables it adds,
/// because a file past step 0 never runs it again.
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

CREATE TABLE IF NOT EXISTS events (
  id        INTEGER PRIMARY KEY,
  review_id TEXT REFERENCES reviews(id),
  kind      TEXT NOT NULL,
  actor     TEXT,
  at        TEXT NOT NULL,
  attrs     TEXT
);
CREATE INDEX IF NOT EXISTS events_review ON events(review_id, id);

DROP TABLE IF EXISTS settings;
"#;

/// One step of the schema's history. The file's `PRAGMA user_version` is
/// how many of these it has been through; opening runs the rest, each in
/// its own transaction with the version bump inside it, so a crash leaves
/// the file at a version it wholly is. A step creates every table it
/// introduces: the baseline is step 0 and a file past it never sees it
/// again.
struct Migration {
    name: &'static str,
    run: fn(&Connection) -> rusqlite::Result<()>,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        name: "the tables",
        run: |conn| conn.execute_batch(SCHEMA),
    },
    Migration {
        name: "one outcomes table for decisions, withdrawals and discards",
        run: migrate_outcomes,
    },
    Migration {
        name: "installed plugins in place of plugin directories",
        run: migrate_installed_plugins,
    },
    Migration {
        name: "the exact plugin version on a review",
        run: |conn| conn.execute_batch("ALTER TABLE reviews ADD COLUMN plugin_release TEXT"),
    },
];

/// The schema as this build writes it; `PRAGMA user_version` on the file.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Brings the file up to this build's schema. A file from a newer build is
/// refused rather than misread.
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_SCHEMA),
            Some(format!(
                "the database is at schema version {version}, newer than this build's {SCHEMA_VERSION}"
            )),
        ));
    }
    for (i, step) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.unchecked_transaction()?;
        (step.run)(&tx)?;
        tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1))?;
        tx.commit()?;
        if version > 0 {
            eprintln!(
                "wicket: database migrated to version {}: {}",
                i + 1,
                step.name
            );
        }
    }
    Ok(())
}

/// One `outcomes` table in place of `decisions`, `withdrawals` and
/// `discards`, so a review ends once whichever way. Rows are copied
/// earliest first; a review that had ended twice (a race the old tables
/// allowed) keeps its first ending and the rest are logged. A file that
/// never had the old tables passes through untouched.
fn migrate_outcomes(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS outcomes (
  review_id  TEXT PRIMARY KEY REFERENCES reviews(id),
  kind       TEXT NOT NULL CHECK (kind IN ('decided', 'withdrawn', 'discarded')),
  at         TEXT NOT NULL,
  by         TEXT,
  reason     TEXT,
  data       TEXT,
  agent_note TEXT
);
CREATE INDEX IF NOT EXISTS outcomes_kind ON outcomes(kind);
",
    )?;
    let has = |table: &str| -> rusqlite::Result<bool> {
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table],
            |_| Ok(()),
        )
        .optional()
        .map(|r| r.is_some())
    };
    let mut rows: Vec<(
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = Vec::new();
    if has("decisions")? {
        let mut stmt = conn
            .prepare("SELECT review_id, decided_at, decided_by, data, agent_note FROM decisions")?;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                "decided".to_string(),
                r.get(1)?,
                r.get::<_, Option<String>>(2)?,
                None,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })? {
            rows.push(row?);
        }
    }
    if has("withdrawals")? {
        let mut stmt = conn.prepare("SELECT review_id, withdrawn_at, reason FROM withdrawals")?;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                "withdrawn".to_string(),
                r.get(1)?,
                None,
                r.get::<_, Option<String>>(2)?,
                None,
                None,
            ))
        })? {
            rows.push(row?);
        }
    }
    if has("discards")? {
        let mut stmt =
            conn.prepare("SELECT review_id, discarded_at, discarded_by, reason FROM discards")?;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                "discarded".to_string(),
                r.get(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                None,
                None,
            ))
        })? {
            rows.push(row?);
        }
    }
    rows.sort_by(|a, b| a.2.cmp(&b.2));
    for (review_id, kind, at, by, reason, data, agent_note) in rows {
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO outcomes (review_id, kind, at, by, reason, data, agent_note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![review_id, kind, at, by, reason, data, agent_note],
        )?;
        if inserted == 0 {
            eprintln!(
                "wicket: review {review_id} had ended twice; its {kind} at {at} is dropped, the earlier ending stands"
            );
        }
    }
    conn.execute_batch(
        "DROP TABLE IF EXISTS decisions; DROP TABLE IF EXISTS withdrawals; DROP TABLE IF EXISTS discards;",
    )?;
    Ok(())
}

/// Plugin directories become links: every plugin that was found inside a
/// registered directory keeps working, served live from where it is, as
/// a linked entry in `installed_plugins`. The first of two plugins with
/// one name wins, as the registry decided before.
fn migrate_installed_plugins(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS installed_plugins (
  name         TEXT PRIMARY KEY,
  version      TEXT NOT NULL,
  major        INTEGER NOT NULL,
  kind         TEXT NOT NULL CHECK (kind IN ('path', 'git', 'release')),
  source       TEXT NOT NULL,
  resolved     TEXT NOT NULL,
  commit_id    TEXT,
  asset_hash   TEXT,
  hash         TEXT,
  build_log    TEXT,
  installed_at TEXT NOT NULL,
  linked       INTEGER NOT NULL DEFAULT 0,
  path         TEXT NOT NULL
);
",
    )?;
    let has_dirs: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'plugin_dirs'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !has_dirs {
        return Ok(());
    }
    let dirs: Vec<String> = conn
        .prepare("SELECT path FROM plugin_dirs ORDER BY added_at, path")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for dir in dirs {
        for sub in crate::plugins::plugin_subdirs(std::path::Path::new(&dir)) {
            let Some(record) = InstalledRecord::linked(&sub) else {
                continue;
            };
            conn.execute(
                "INSERT OR IGNORE INTO installed_plugins (name, version, major, kind, source, resolved, installed_at, linked, path)
                 VALUES (?1, ?2, ?3, 'path', ?4, ?4, ?5, 1, ?4)",
                params![record.name, record.version, record.major, record.path, record.installed_at],
            )?;
        }
    }
    conn.execute_batch("DROP TABLE plugin_dirs")?;
    Ok(())
}

/// One installed plugin: a linked folder served live, or an entry in the
/// store, with where it came from and what was placed.
#[derive(Debug, Clone)]
pub struct InstalledRecord {
    pub name: String,
    pub version: String,
    pub major: i64,
    /// `path`, `git` or `release`
    pub kind: String,
    pub source: String,
    pub resolved: String,
    pub commit: Option<String>,
    pub asset_hash: Option<String>,
    /// SHA-256 over the placed files; none for a link
    pub hash: Option<String>,
    pub build_log: Option<String>,
    pub installed_at: String,
    pub linked: bool,
    /// the folder for a link, the store entry otherwise
    pub path: String,
}

impl InstalledRecord {
    /// A link to a plugin folder as it is: the manifest names it.
    pub fn linked(dir: &std::path::Path) -> Option<InstalledRecord> {
        let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
        let manifest: Value = serde_json::from_str(&text).ok()?;
        let name = manifest.get("name")?.as_str()?.to_string();
        let (version, major) = crate::plugins::version_of(manifest.get("version")?)?;
        let path = std::path::absolute(dir).ok()?.display().to_string();
        Some(InstalledRecord {
            name,
            version,
            major,
            kind: "path".into(),
            source: path.clone(),
            resolved: path.clone(),
            commit: None,
            asset_hash: None,
            hash: None,
            build_log: None,
            installed_at: crate::review::iso(Utc::now()),
            linked: true,
            path,
        })
    }
}

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
        migrate(&conn)?;
        Ok(Db {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert_review(&self, review: &Review, actor: Option<&str>) -> rusqlite::Result<i64> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO reviews (id, plugin, plugin_version, plugin_release, title, origin, requested_by, payload, summary, revises, expires_at, created_at)
             VALUES (?1, ?2, ?3, ?12, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
                review.plugin_release,
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

    /// Reviews matching the filters, newest first, without payloads.
    pub fn list(&self, filters: &Filters, now: DateTime<Utc>) -> rusqlite::Result<Vec<Review>> {
        let (mut sql, mut args) = self.where_clause(filters, now);
        sql = format!("{SELECT}{sql} ORDER BY r.id DESC");
        if filters.limit > 0 {
            args.push(filters.limit.to_string());
            sql.push_str(&format!(" LIMIT ?{}", args.len()));
        }
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            args.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(params.as_slice(), |row| row_to_review(row, false))?;
        rows.collect()
    }

    /// How many reviews match the filters; the limit and cursor play no part.
    pub fn count(&self, filters: &Filters, now: DateTime<Utc>) -> rusqlite::Result<usize> {
        let mut filters = filters.clone();
        filters.cursor = None;
        let (sql, args) = self.where_clause(&filters, now);
        let conn = self.conn.lock().unwrap();
        let params: Vec<&dyn rusqlite::ToSql> =
            args.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM reviews r LEFT JOIN outcomes o ON o.review_id = r.id{sql}"
            ),
            params.as_slice(),
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n as usize)
    }

    /// The `WHERE` for the filters, status included, with its parameters.
    fn where_clause(&self, filters: &Filters, now: DateTime<Utc>) -> (String, Vec<String>) {
        let mut sql = " WHERE 1=1".to_string();
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
        if !filters.statuses.is_empty() {
            // only pending and expired read the clock; a bound parameter
            // nothing references is an error to SQLite
            let clocked = filters
                .statuses
                .iter()
                .any(|s| matches!(s, Status::Pending | Status::Expired));
            let n = if clocked {
                push(&crate::review::iso(now))
            } else {
                0
            };
            let any: Vec<String> = filters.statuses.iter().map(|s| status_sql(*s, n)).collect();
            sql.push_str(&format!(" AND ({})", any.join(" OR ")));
        }
        (sql, args)
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
                crate::review::iso(outcome.at),
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
               AND o.review_id IS NULL
               AND NOT EXISTS (SELECT 1 FROM events e WHERE e.review_id = r.id AND e.kind = 'expired')
             ORDER BY r.id"
        ))?;
        let rows = stmt.query_map(params![crate::review::iso(now)], |row| {
            row_to_review(row, false)
        })?;
        rows.collect()
    }

    pub fn installed_plugins(&self) -> rusqlite::Result<Vec<InstalledRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT name, version, major, kind, source, resolved, commit_id, asset_hash, hash, build_log, installed_at, linked, path
             FROM installed_plugins ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(InstalledRecord {
                name: r.get(0)?,
                version: r.get(1)?,
                major: r.get(2)?,
                kind: r.get(3)?,
                source: r.get(4)?,
                resolved: r.get(5)?,
                commit: r.get(6)?,
                asset_hash: r.get(7)?,
                hash: r.get(8)?,
                build_log: r.get(9)?,
                installed_at: r.get(10)?,
                linked: r.get::<_, i64>(11)? != 0,
                path: r.get(12)?,
            })
        })?;
        rows.collect()
    }

    /// Writes the record, replacing the plugin's previous one.
    pub fn upsert_installed(&self, record: &InstalledRecord) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO installed_plugins
             (name, version, major, kind, source, resolved, commit_id, asset_hash, hash, build_log, installed_at, linked, path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record.name,
                record.version,
                record.major,
                record.kind,
                record.source,
                record.resolved,
                record.commit,
                record.asset_hash,
                record.hash,
                record.build_log,
                record.installed_at,
                record.linked as i64,
                record.path
            ],
        )?;
        Ok(())
    }

    /// Whether any review renders from this plugin's line.
    pub fn reviews_use(&self, plugin: &str, major: u32) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT 1 FROM reviews WHERE plugin = ?1 AND plugin_version = ?2 LIMIT 1",
            params![plugin, major],
            |_| Ok(()),
        )
        .optional()
        .map(|r| r.is_some())
    }

    pub fn remove_installed(&self, name: &str) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute(
            "DELETE FROM installed_plugins WHERE name = ?1",
            params![name],
        )? > 0)
    }
}

const SELECT: &str = "SELECT r.id, r.plugin, r.plugin_version, r.title, r.origin, r.requested_by, r.payload, r.summary,
    r.revises, r.expires_at, r.created_at,
    o.kind, o.at, o.by, o.reason, o.data, o.agent_note,
    r.plugin_release
  FROM reviews r
  LEFT JOIN outcomes o ON o.review_id = r.id";

/// What `Review::status` says, as SQL over the same columns, so a listing
/// and a count read only the rows they answer with. `?now` is the caller's
/// parameter number for the current time.
fn status_sql(status: Status, now: usize) -> String {
    match status {
        Status::Decided => "o.kind = 'decided'".to_string(),
        Status::Withdrawn => "o.kind = 'withdrawn'".to_string(),
        Status::Discarded => "o.kind = 'discarded'".to_string(),
        Status::Pending => {
            format!("(o.review_id IS NULL AND (r.expires_at IS NULL OR r.expires_at > ?{now}))")
        }
        Status::Expired => {
            format!("(o.review_id IS NULL AND r.expires_at IS NOT NULL AND r.expires_at <= ?{now})")
        }
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
    // the outcome, if the review has ended: one row, its kind says which
    // fields it fills
    let kind: Option<String> = row.get(11)?;
    let at: Option<String> = row.get(12)?;
    let by: Option<String> = row.get(13)?;
    let reason: Option<String> = row.get(14)?;
    let data: Option<String> = row.get(15)?;
    let agent_note: Option<String> = row.get(16)?;
    let plugin_release: Option<String> = row.get(17)?;
    let plugin_version = row.get::<_, i64>(2)? as u32;
    let at = at.and_then(|s| parse_datetime(&s));
    let (
        mut decision,
        mut withdrawn_at,
        mut withdrawn_reason,
        mut discarded_at,
        mut discarded_by,
        mut discarded_reason,
    ) = (None, None, None, None, None, None);
    match kind.as_deref() {
        Some("decided") => {
            decision = Some(Decision {
                decided_by: by.clone().unwrap_or_default(),
                decided_at: at.unwrap_or_default(),
                data: data
                    .as_deref()
                    .and_then(|d| serde_json::from_str(d).ok())
                    .unwrap_or(Value::Null),
            })
        }
        Some("withdrawn") => {
            withdrawn_at = at;
            withdrawn_reason = reason.clone();
        }
        Some("discarded") => {
            discarded_at = at;
            discarded_by = by.clone();
            discarded_reason = reason.clone();
        }
        _ => {}
    }
    Ok(Review {
        id: row.get(0)?,
        plugin: row.get(1)?,
        plugin_version,
        plugin_release: plugin_release.unwrap_or_else(|| format!("{plugin_version}.0.0")),
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
        agent_note: if kind.as_deref() == Some("decided") {
            agent_note
        } else {
            None
        },
        withdrawn_at,
        withdrawn_reason,
        discarded_at,
        discarded_by,
        discarded_reason,
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
