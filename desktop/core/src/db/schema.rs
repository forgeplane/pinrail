//! Migrations, the way Ecto keeps them: one file per change under
//! `desktop/core/migrations/`, named `<version>_<name>.sql` where the
//! version is the UTC moment it was written (`20260923193000`), and a
//! `schema_migrations` table with a row per migration applied.
//!
//! Opening runs every migration the table has no row for, in version order,
//! each in its own transaction with its row inserted inside it, so a crash
//! leaves the file with a migration wholly applied or not at all. A
//! migration merged late with an older version than one already applied
//! still runs, as in Ecto. A file with a row this build has no migration
//! for was written by a newer build, and is refused rather than misread.
//!
//! A migration that has to move data rather than change the schema is a
//! Rust function instead of a file; it is listed here all the same.
//!
//! Never edit a migration once it is merged: add another.

use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, params};

pub(super) struct Migration {
    /// `YYYYMMDDHHMMSS`, UTC, as in the file's name.
    pub version: i64,
    pub name: &'static str,
    pub step: Step,
}

pub(super) enum Step {
    Sql(&'static str),
    #[allow(dead_code)] // none yet; the first data migration will use it
    Rust(fn(&Connection) -> rusqlite::Result<()>),
}

/// Every migration, oldest first. A test checks this against the files.
pub(super) const MIGRATIONS: &[Migration] = &[Migration {
    version: 20260923193000,
    name: "baseline",
    step: Step::Sql(include_str!("../../migrations/20260923193000_baseline.sql")),
}];

/// The newest migration this build knows.
pub const LATEST_MIGRATION: i64 = MIGRATIONS[MIGRATIONS.len() - 1].version;

/// The last `PRAGMA user_version` of the numbered steps that came before
/// the table, which the baseline squashes: a file there has the baseline's
/// schema already.
const LAST_NUMBERED_STEP: i64 = 4;

/// Brings the file up to this build's schema.
pub(super) fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    adopt(conn)?;
    run(conn, MIGRATIONS)
}

pub(super) fn run(conn: &Connection, migrations: &[Migration]) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
  version     INTEGER PRIMARY KEY,
  inserted_at TEXT
);",
    )?;
    let applied: Vec<i64> = conn
        .prepare("SELECT version FROM schema_migrations ORDER BY version")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    if let Some(unknown) = applied
        .iter()
        .find(|v| !migrations.iter().any(|m| m.version == **v))
    {
        return Err(refused(format!(
            "the database has migration {unknown}, which this build does not know: it was opened by a newer Pinrail"
        )));
    }
    let fresh = applied.is_empty();
    let mut pending: Vec<&Migration> = migrations
        .iter()
        .filter(|m| !applied.contains(&m.version))
        .collect();
    pending.sort_by_key(|m| m.version);
    for migration in pending {
        let tx = conn.unchecked_transaction()?;
        match migration.step {
            Step::Sql(sql) => tx.execute_batch(sql)?,
            Step::Rust(step) => step(&tx)?,
        }
        tx.execute(
            "INSERT INTO schema_migrations (version, inserted_at) VALUES (?1, ?2)",
            params![
                migration.version,
                Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
            ],
        )?;
        tx.commit()?;
        if !fresh {
            eprintln!(
                "pinrail: database migrated: {}_{}",
                migration.version, migration.name
            );
        }
    }
    Ok(())
}

/// A file from before the table: at the last numbered step it already has
/// the baseline's schema, so the baseline is recorded rather than run, and
/// `user_version` goes back to 0. A file at an earlier step, or with tables
/// and no version at all, predates the baseline and is refused.
fn adopt(conn: &Connection) -> rusqlite::Result<()> {
    let has_table = |name: &str| -> rusqlite::Result<bool> {
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |_| Ok(()),
        )
        .optional()
        .map(|r| r.is_some())
    };
    if has_table("schema_migrations")? {
        return Ok(());
    }
    let numbered: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    match numbered {
        0 if !has_table("reviews")? => Ok(()),
        LAST_NUMBERED_STEP => {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(
                "CREATE TABLE schema_migrations (
  version     INTEGER PRIMARY KEY,
  inserted_at TEXT
);",
            )?;
            tx.execute(
                "INSERT INTO schema_migrations (version, inserted_at) VALUES (?1, ?2)",
                params![
                    MIGRATIONS[0].version,
                    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
                ],
            )?;
            tx.execute_batch("PRAGMA user_version = 0")?;
            tx.commit()?;
            eprintln!(
                "pinrail: database adopted into schema_migrations at {}_{}",
                MIGRATIONS[0].version, MIGRATIONS[0].name
            );
            Ok(())
        }
        other => Err(refused(format!(
            "the database is from before 2026-09-23 (schema step {other}), older than any Pinrail release can read"
        ))),
    }
}

fn refused(message: String) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_SCHEMA),
        Some(message),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn versions(conn: &Connection) -> Vec<i64> {
        conn.prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn table(version: i64, name: &'static str, sql: &'static str) -> Migration {
        Migration {
            version,
            name,
            step: Step::Sql(sql),
        }
    }

    #[test]
    fn every_file_is_listed_once_in_version_order() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        files.sort();
        let sql: Vec<String> = MIGRATIONS
            .iter()
            .filter(|m| matches!(m.step, Step::Sql(_)))
            .map(|m| format!("{}_{}.sql", m.version, m.name))
            .collect();
        assert_eq!(
            files, sql,
            "every file in migrations/ is in MIGRATIONS, by its name"
        );
        for pair in MIGRATIONS.windows(2) {
            assert!(
                pair[0].version < pair[1].version,
                "{} before {}",
                pair[0].version,
                pair[1].version
            );
        }
        for m in MIGRATIONS {
            let v = m.version.to_string();
            assert_eq!(v.len(), 14, "{v} is YYYYMMDDHHMMSS");
            assert!(
                chrono::NaiveDateTime::parse_from_str(&v, "%Y%m%d%H%M%S").is_ok(),
                "{v}"
            );
        }
    }

    #[test]
    fn pending_migrations_run_in_version_order_once_each() {
        let conn = Connection::open_in_memory().unwrap();
        let first = [table(20260101000000, "a", "CREATE TABLE a (x);")];
        run(&conn, &first).unwrap();
        // a branch's migration merged late, older than one already applied, still runs
        let later = [
            table(20260101000000, "a", "CREATE TABLE a (x);"),
            table(20251231000000, "late", "CREATE TABLE late (x);"),
            table(20260201000000, "b", "CREATE TABLE b (x);"),
        ];
        run(&conn, &later).unwrap();
        run(&conn, &later).unwrap();
        assert_eq!(
            versions(&conn),
            [20251231000000, 20260101000000, 20260201000000]
        );
        let at: String = conn
            .query_row(
                "SELECT inserted_at FROM schema_migrations WHERE version = 20260201000000",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(at.ends_with('Z'), "{at}");
    }

    #[test]
    fn a_failing_migration_leaves_neither_its_changes_nor_its_row() {
        let conn = Connection::open_in_memory().unwrap();
        let broken = [
            table(20260101000000, "a", "CREATE TABLE a (x);"),
            table(
                20260102000000,
                "half",
                "CREATE TABLE half (x); THIS IS NOT SQL;",
            ),
        ];
        assert!(run(&conn, &broken).is_err());
        assert_eq!(versions(&conn), [20260101000000]);
        let half: Option<i64> = conn
            .query_row("SELECT 1 FROM sqlite_master WHERE name = 'half'", [], |r| {
                r.get(0)
            })
            .optional()
            .unwrap();
        assert!(half.is_none());
    }

    #[test]
    fn a_migration_this_build_does_not_know_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        run(&conn, &[table(20260101000000, "a", "CREATE TABLE a (x);")]).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (20990101000000, NULL)",
            [],
        )
        .unwrap();
        let error = run(&conn, &[table(20260101000000, "a", "CREATE TABLE a (x);")])
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("20990101000000") && error.contains("newer Pinrail"),
            "{error}"
        );
    }

    #[test]
    fn a_rust_step_runs_like_a_file() {
        let conn = Connection::open_in_memory().unwrap();
        let steps = [
            table(
                20260101000000,
                "a",
                "CREATE TABLE a (x); INSERT INTO a VALUES (1);",
            ),
            Migration {
                version: 20260102000000,
                name: "double",
                step: Step::Rust(|c| c.execute_batch("UPDATE a SET x = x * 2")),
            },
        ];
        run(&conn, &steps).unwrap();
        let x: i64 = conn.query_row("SELECT x FROM a", [], |r| r.get(0)).unwrap();
        assert_eq!(x, 2);
    }
}
