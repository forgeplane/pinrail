//! The tables, and the numbered steps that bring an older file up to date.
//! A step runs once; the file records how far it has come.

use rusqlite::{Connection, OptionalExtension, params};

use super::InstalledRecord;

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
pub(super) fn migrate(conn: &Connection) -> rusqlite::Result<()> {
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
    // review_id, at, by, data, note, reason, discarded_by: one row per
    // outcome from any of the three old tables
    type OldOutcome = (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut rows: Vec<OldOutcome> = Vec::new();
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
