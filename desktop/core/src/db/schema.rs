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
//! The files are the list: `build.rs` embeds every one, and nothing here
//! names them.
//!
//! Never edit a migration once it is merged: add another.

use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, params};

/// A migration: its version, its name and its SQL.
pub(super) type Migration = (i64, &'static str, &'static str);

/// Every file in `migrations/`, oldest first, embedded by `build.rs`.
pub(super) const MIGRATIONS: &[Migration] = include!(concat!(env!("OUT_DIR"), "/migrations.rs"));

/// The newest migration this build knows.
pub const LATEST_MIGRATION: i64 = MIGRATIONS[MIGRATIONS.len() - 1].0;

/// Brings the file up to this build's schema.
pub(super) fn migrate(conn: &Connection) -> rusqlite::Result<()> {
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
        .find(|v| !migrations.iter().any(|m| m.0 == **v))
    {
        return Err(refused(format!(
            "the database has migration {unknown}, which this build does not know: it was opened by a newer Pinrail"
        )));
    }
    let fresh = applied.is_empty();
    let mut pending: Vec<&Migration> = migrations
        .iter()
        .filter(|m| !applied.contains(&m.0))
        .collect();
    pending.sort_by_key(|m| m.0);
    for &&(version, name, sql) in &pending {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, inserted_at) VALUES (?1, ?2)",
            params![
                version,
                Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
            ],
        )?;
        tx.commit()?;
        if !fresh {
            eprintln!("pinrail: database migrated: {version}_{name}");
        }
    }
    Ok(())
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
    use rusqlite::OptionalExtension;

    fn versions(conn: &Connection) -> Vec<i64> {
        conn.prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn every_file_is_embedded_in_version_order() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        files.sort();
        let embedded: Vec<String> = MIGRATIONS
            .iter()
            .map(|(v, n, _)| format!("{v}_{n}.sql"))
            .collect();
        assert_eq!(files, embedded);
        assert!(MIGRATIONS.windows(2).all(|p| p[0].0 < p[1].0));
        assert!(MIGRATIONS[0].2.contains("CREATE TABLE reviews"));
    }

    #[test]
    fn pending_migrations_run_in_version_order_once_each() {
        let conn = Connection::open_in_memory().unwrap();
        run(&conn, &[(20260101000000, "a", "CREATE TABLE a (x);")]).unwrap();
        // a branch's migration merged late, older than one already applied, still runs
        let later = [
            (20251231000000, "late", "CREATE TABLE late (x);"),
            (20260101000000, "a", "CREATE TABLE a (x);"),
            (20260201000000, "b", "CREATE TABLE b (x);"),
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
            (20260101000000, "a", "CREATE TABLE a (x);"),
            (
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
        let known = [(20260101000000, "a", "CREATE TABLE a (x);")];
        run(&conn, &known).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (20990101000000, NULL)",
            [],
        )
        .unwrap();
        let error = run(&conn, &known).unwrap_err().to_string();
        assert!(
            error.contains("20990101000000") && error.contains("newer Pinrail"),
            "{error}"
        );
    }
}
