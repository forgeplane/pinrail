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

use chrono::Utc;
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
    register(conn)?;
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
            params![version, crate::reviews::iso(Utc::now())],
        )?;
        tx.commit()?;
        if !fresh {
            eprintln!("pinrail: database migrated: {version}_{name}");
        }
    }
    Ok(())
}

/// Adds `new_session_id()`, which a migration that gives sessions Pinrail's
/// ids calls once per row.
fn register(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function(
        "new_session_id",
        0,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8,
        |_| Ok(crate::id::next_session()),
    )
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

    /// The reviews stored before questions are reviews of the kind
    /// `review`, and only a question has no plugin.
    #[test]
    fn a_question_alone_has_no_plugin() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(MIGRATIONS.iter().any(|m| m.1 == "review_kind"));
        let before: Vec<Migration> = MIGRATIONS
            .iter()
            .filter(|m| m.1 != "review_kind")
            .copied()
            .collect();
        run(&conn, &before).unwrap();
        let insert = |id: &str, plugin: &str, kind: Option<&str>| {
            let sql = match kind {
                Some(_) => {
                    "INSERT INTO reviews (id, plugin, title, origin, created_at, kind) \
                            VALUES (?1, ?2, 't', '{}', '2026-10-29T00:00:00Z', ?3)"
                }
                None => {
                    "INSERT INTO reviews (id, plugin, title, origin, created_at) \
                         VALUES (?1, ?2, 't', '{}', '2026-10-29T00:00:00Z')"
                }
            };
            match kind {
                Some(kind) => conn.execute(sql, params![id, plugin, kind]),
                None => conn.execute(sql, params![id, plugin]),
            }
        };
        insert("r_old", "list", None).unwrap();
        run(&conn, MIGRATIONS).unwrap();

        let kind: String = conn
            .query_row("SELECT kind FROM reviews WHERE id = 'r_old'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(kind, "review");
        insert("r_question", "", Some("question")).unwrap();
        assert!(insert("r_no_plugin", "", Some("review")).is_err());
        assert!(insert("r_plugin_question", "list", Some("question")).is_err());
        assert!(insert("r_other", "list", Some("message")).is_err());
    }

    /// The sessions the reviews named before the table are its rows, with
    /// the newest review's project and branch, and the time of their newest
    /// event other than a review being viewed, to the millisecond.
    #[test]
    fn the_sessions_are_filled_from_the_reviews() {
        let conn = Connection::open_in_memory().unwrap();
        let before: Vec<Migration> = MIGRATIONS
            .iter()
            .filter(|m| m.0 < 20261030120000)
            .copied()
            .collect();
        run(&conn, &before).unwrap();
        let review = |id: &str,
                      agent: Option<&str>,
                      session: Option<&str>,
                      origin: &str,
                      at: &str| {
            conn.execute(
                "INSERT INTO reviews (id, plugin, title, origin, requested_by, session, created_at) \
                 VALUES (?1, 'list', 't', ?2, ?3, ?4, ?5)",
                params![id, origin, agent, session, at],
            )
            .unwrap();
        };
        let event = |review: &str, kind: &str, at: &str| {
            conn.execute(
                "INSERT INTO events (review_id, kind, at) VALUES (?1, ?2, ?3)",
                params![review, kind, at],
            )
            .unwrap();
        };
        review(
            "r1",
            Some("claude-code"),
            Some("s1"),
            r#"{"repo":"acme/api","ref":"main"}"#,
            "2026-10-01T10:00:00Z",
        );
        review(
            "r2",
            Some("claude-code"),
            Some("s1"),
            r#"{"repo":"acme/api","ref":"fix"}"#,
            "2026-10-02T10:00:00Z",
        );
        review(
            "r3",
            Some("codex"),
            Some("s1"),
            "{}",
            "2026-10-03T10:00:00Z",
        );
        review(
            "r4",
            None,
            None,
            r#"{"repo":"acme/web"}"#,
            "2026-10-04T10:00:00Z",
        );
        event("r1", "created", "2026-10-01T10:00:00Z");
        event("r2", "created", "2026-10-02T10:00:00Z");
        event("r1", "decided", "2026-10-05T10:00:00Z");
        event("r2", "viewed", "2026-10-09T10:00:00Z");
        event("r3", "created", "2026-10-03T10:00:00Z");
        run(&conn, MIGRATIONS).unwrap();

        let rows: Vec<String> = conn
            .prepare(
                "SELECT agent || ' ' || s.agent_session || ' ' || IFNULL(p.name, '-') || ' ' || IFNULL(ref, '-') \
                 || ' ' || IFNULL(folder, '-') || ' ' || first_at || ' ' || last_at \
                 FROM sessions s LEFT JOIN repos p ON p.id = s.repo_id ORDER BY agent",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            [
                "claude-code s1 acme/api fix - 2026-10-01T10:00:00.000000Z 2026-10-05T10:00:00.000000Z",
                "codex s1 - - - 2026-10-03T10:00:00.000000Z 2026-10-03T10:00:00.000000Z",
            ]
        );
    }

    /// Every repository the reviews and the sessions name gets a row, from
    /// the first time it was named, and each review and session refers to
    /// it; a review or a session that names none refers to none.
    #[test]
    fn the_repositories_are_filled_from_the_reviews_and_sessions() {
        let conn = Connection::open_in_memory().unwrap();
        let before: Vec<Migration> = MIGRATIONS
            .iter()
            .filter(|m| m.0 < 20261105120000)
            .copied()
            .collect();
        run(&conn, &before).unwrap();
        conn.execute_batch(
            r#"INSERT INTO reviews (id, plugin, title, origin, created_at) VALUES
                 ('r1', 'list', 't', '{"repo":"acme/api","ref":"main"}', '2026-10-02T10:00:00Z'),
                 ('r2', 'list', 't', '{"repo":"acme/api","ref":"fix"}', '2026-10-01T10:00:00Z'),
                 ('r3', 'list', 't', '{"repo":"acme/web"}', '2026-10-03T10:00:00Z'),
                 ('r4', 'list', 't', '{}', '2026-10-04T10:00:00Z'),
                 ('r5', 'list', 't', '{"repo":""}', '2026-10-05T10:00:00Z');
               INSERT INTO sessions (agent, id, repo, first_at, last_at) VALUES
                 ('claude-code', 's1', 'acme/api', '2026-09-30T10:00:00Z', '2026-10-01T00:00:00.000Z'),
                 ('codex', 's2', 'acme/docs', '2026-10-06T10:00:00Z', '2026-10-06T00:00:00.000Z'),
                 ('codex', 's3', NULL, '2026-10-07T10:00:00Z', '2026-10-07T00:00:00.000Z');"#,
        )
        .unwrap();
        run(&conn, MIGRATIONS).unwrap();

        let rows = |sql: &str| -> Vec<String> {
            conn.prepare(sql)
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(
            rows(
                "SELECT name || ' ' || created_at || ' ' || last_item || ' ' \
                  || IFNULL(project_id, '-') || ' ' || IFNULL(prefix, '-') FROM repos ORDER BY name"
            ),
            [
                "acme/api 2026-09-30T10:00:00.000000Z 0 - -",
                "acme/docs 2026-10-06T10:00:00.000000Z 0 - -",
                "acme/web 2026-10-03T10:00:00.000000Z 0 - -",
            ]
        );
        assert_eq!(
            rows(
                "SELECT r.id || ' ' || IFNULL(p.name, '-') FROM reviews r \
                  LEFT JOIN repos p ON p.id = r.repo_id ORDER BY r.id"
            ),
            ["r1 acme/api", "r2 acme/api", "r3 acme/web", "r4 -", "r5 -"]
        );
        assert_eq!(
            rows(
                "SELECT s.agent_session || ' ' || IFNULL(p.name, '-') FROM sessions s \
                  LEFT JOIN repos p ON p.id = s.repo_id ORDER BY s.agent_session"
            ),
            ["s1 acme/api", "s2 acme/docs", "s3 -"]
        );
        // the name lives only in the repository's row now
        let columns = rows("SELECT name FROM pragma_table_info('sessions')");
        assert!(!columns.contains(&"repo".to_string()), "{columns:?}");
        // deleting a repository deletes its reviews and sessions
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        conn.execute("DELETE FROM repos WHERE name = 'acme/api'", [])
            .unwrap();
        assert_eq!(
            rows("SELECT id FROM reviews ORDER BY id"),
            ["r3", "r4", "r5"]
        );
        assert_eq!(
            rows("SELECT agent_session FROM sessions ORDER BY agent_session"),
            ["s2", "s3"]
        );
    }

    /// A status of the first sessions table keeps its id and text, and
    /// says what it was: the end of the agent's work, or a step.
    #[test]
    fn the_statuses_are_given_their_kind() {
        let conn = Connection::open_in_memory().unwrap();
        let before: Vec<Migration> = MIGRATIONS
            .iter()
            .filter(|m| m.0 < 20261030150000)
            .copied()
            .collect();
        run(&conn, &before).unwrap();
        conn.execute_batch(
            "INSERT INTO sessions (agent, id, first_at, last_at) \
             VALUES ('claude-code', 's1', '2026-10-30T10:00:00Z', '2026-10-30T10:00:00.000Z'); \
             INSERT INTO session_statuses (id, agent, session, text, done, created_at) VALUES \
               (7, 'claude-code', 's1', 'Testing', 0, '2026-10-30T10:00:00Z'), \
               (9, 'claude-code', 's1', 'Pushed', 1, '2026-10-30T10:01:00Z');",
        )
        .unwrap();
        run(&conn, MIGRATIONS).unwrap();
        let rows: Vec<String> = conn
            .prepare("SELECT id || ' ' || text || ' ' || kind FROM session_statuses ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows, ["7 Testing step", "9 Pushed done"]);
        // and a session's statuses still go with it
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        conn.execute("DELETE FROM sessions", []).unwrap();
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM session_statuses", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }

    /// A database as 0.6.0 left it keeps every session, shown, with what
    /// hangs off it, and no presence, which is unknown until a hook reports.
    #[test]
    fn sessions_from_before_presence_stay_shown_without_a_state() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        let before: Vec<Migration> = MIGRATIONS
            .iter()
            .filter(|m| m.0 < 20261113120000)
            .copied()
            .collect();
        run(&conn, &before).unwrap();
        conn.execute_batch(
            "INSERT INTO sessions (agent, id, first_at, last_at, archived_at) VALUES \
               ('claude-code', 's1', '2026-11-01T10:00:00.000000Z', '2026-11-01T10:00:00.000000Z', NULL), \
               ('codex', 's2', '2026-11-02T10:00:00.000000Z', '2026-11-02T10:00:00.000000Z', '2026-11-03T10:00:00.000000Z'); \
             INSERT INTO session_statuses (agent, session, text, kind, created_at) VALUES \
               ('claude-code', 's1', 'Testing', 'step', '2026-11-01T10:00:00.000000Z'); \
             INSERT INTO session_messages (agent, session, text, sent_by, sent_at) VALUES \
               ('codex', 's2', 'Rebase first', 'you', '2026-11-02T10:00:00.000000Z');",
        )
        .unwrap();
        run(&conn, MIGRATIONS).unwrap();
        let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(count("SELECT COUNT(*) FROM sessions WHERE hidden = 0"), 2);
        assert_eq!(count("SELECT COUNT(*) FROM session_statuses"), 1);
        assert_eq!(count("SELECT COUNT(*) FROM session_messages"), 1);
        assert_eq!(count("SELECT COUNT(*) FROM session_presence"), 0);
        // a flag is 0 or 1, and a presence belongs to a session
        assert!(conn.execute("UPDATE sessions SET hidden = 2", []).is_err());
        assert!(
            conn.execute(
                "INSERT INTO session_presence (agent, session, state, source, event_at, state_at, state_since, updated_at) \
                 VALUES ('codex', 'gone', 'idle', 'hook', 'x', 'x', 'x', 'x')",
                [],
            )
            .is_err()
        );
    }

    /// Each session gets an id of Pinrail's own and keeps the agent's as
    /// `agent_session`, and everything that named it by the agent's id
    /// names it by Pinrail's, including what stored events carry. A
    /// session named without a row of its own gets one, hidden.
    #[test]
    fn sessions_get_pinrails_ids_and_keep_the_agents() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        let before: Vec<Migration> = MIGRATIONS
            .iter()
            .filter(|m| m.0 < 20261120120000)
            .copied()
            .collect();
        run(&conn, &before).unwrap();
        let t = "'2026-11-01T10:00:00.000000Z'";
        conn.execute_batch(&format!(
            r#"INSERT INTO repos (id, name, prefix, created_at) VALUES (1, 'acme/api', 'API', {t});
             INSERT INTO sessions (agent, id, first_at, last_at) VALUES
               ('claude-code', 's1', {t}, {t}), ('codex', 's1', {t}, {t});
             INSERT INTO session_statuses (agent, session, text, kind, created_at)
               VALUES ('claude-code', 's1', 'Testing', 'step', {t});
             INSERT INTO session_messages (agent, session, text, sent_by, sent_at)
               VALUES ('codex', 's1', 'Rebase first', 'you', {t});
             INSERT INTO session_presence (agent, session, state, source, event_at, state_at,
               state_since, updated_at) VALUES ('claude-code', 's1', 'idle', 'hook', {t}, {t}, {t}, {t});
             INSERT INTO session_turns (agent, session, turn, started_at)
               VALUES ('claude-code', 's1', 1, {t});
             INSERT INTO runs (id, host_id, agent, kind, folder, state, created_by, started_at)
               VALUES ('run_1', 'local', 'codex', 'start', '/w', 'ended', 'app', {t});
             INSERT INTO run_sessions (run_id, agent, session, bound_at)
               VALUES ('run_1', 'codex', 's1', {t});
             INSERT INTO reviews (id, plugin, title, origin, requested_by, session, created_at)
               VALUES ('r1', 'list', 't', '{{}}', 'claude-code', 's1', {t}),
                      ('r2', 'list', 't', '{{}}', 'claude-code', 'gone', {t});
             INSERT INTO outcomes (review_id, kind, at, by, via, session)
               VALUES ('r1', 'withdrawn', {t}, 'claude-code', 'cli', 's1');
             INSERT INTO work_items (id, repo_id, number, title, status, claim_agent, claim_session,
               created_by, created_session, created_at, updated_at)
               VALUES ('a', 1, 1, 'Fix', 'in_progress', 'codex', 's1', 'claude-code', 's1', {t}, {t});
             INSERT INTO work_item_events (item_id, kind, actor, session, at, attrs)
               VALUES ('a', 'claimed', 'codex', 's1', {t},
                       '{{"from":{{"agent":"claude-code","session":"s1"}}}}');
             INSERT INTO work_item_comments (id, item_id, body, author, session, created_at)
               VALUES ('c1', 'a', 'Done', 'codex', 's1', {t});
             INSERT INTO events (kind, at, attrs) VALUES
               ('session_status', {t}, '{{"agent":"claude-code","id":"s1","status":{{}}}}'),
               ('item_changed', {t}, '{{"id":"a","key":"API-1","agent":"codex","session":"s1"}}'),
               ('run_bound', {t}, '{{"run":"run_1","session":{{"agent":"codex","id":"s1"}},"previous":null}}'),
               ('run_ended', {t}, '{{"run":{{"id":"run_1","session":{{"agent":"codex","id":"s1"}}}}}}');"#
        ))
        .unwrap();
        run(&conn, MIGRATIONS).unwrap();

        let one = |sql: &str| -> String { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        let id = |agent: &str, own: &str| -> String {
            conn.query_row(
                "SELECT id FROM sessions WHERE agent = ?1 AND agent_session = ?2",
                params![agent, own],
                |r| r.get(0),
            )
            .unwrap()
        };
        let (claude, codex, gone) = (
            id("claude-code", "s1"),
            id("codex", "s1"),
            id("claude-code", "gone"),
        );
        for id in [&claude, &codex, &gone] {
            assert!(id.starts_with("ses_") && id.len() == 30, "{id}");
        }
        assert_ne!(claude, codex);
        // a session named only by a review is hidden
        assert_eq!(
            one(&format!(
                "SELECT hidden || '' FROM sessions WHERE id = '{gone}'"
            )),
            "1"
        );
        for (sql, expected) in [
            ("SELECT session FROM session_statuses", &claude),
            ("SELECT session FROM session_messages", &codex),
            ("SELECT session FROM session_presence", &claude),
            ("SELECT session FROM session_turns", &claude),
            ("SELECT session FROM run_sessions", &codex),
            ("SELECT session FROM reviews WHERE id = 'r1'", &claude),
            ("SELECT session FROM reviews WHERE id = 'r2'", &gone),
            ("SELECT session FROM outcomes", &claude),
            ("SELECT claim_session FROM work_items", &codex),
            ("SELECT created_session FROM work_items", &claude),
            ("SELECT session FROM work_item_events", &codex),
            ("SELECT session FROM work_item_comments", &codex),
            (
                "SELECT json_extract(attrs, '$.from.session') FROM work_item_events",
                &claude,
            ),
            (
                "SELECT json_extract(attrs, '$.id') FROM events WHERE kind = 'session_status'",
                &claude,
            ),
            (
                "SELECT json_extract(attrs, '$.session') FROM events WHERE kind = 'item_changed'",
                &codex,
            ),
            (
                "SELECT json_extract(attrs, '$.session.id') FROM events WHERE kind = 'run_bound'",
                &codex,
            ),
            (
                "SELECT json_extract(attrs, '$.run.session.id') FROM events WHERE kind = 'run_ended'",
                &codex,
            ),
        ] {
            assert_eq!(&one(sql), expected, "{sql}");
        }
        // the item an event is about keeps its own id
        assert_eq!(
            one("SELECT json_extract(attrs, '$.id') FROM events WHERE kind = 'item_changed'"),
            "a"
        );
        // every reference holds, and an agent's id is one session per agent
        let broken: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(broken, 0);
        assert!(
            conn.execute(
                "INSERT INTO sessions (agent, id, agent_session, first_at, last_at) \
                 VALUES ('codex', 'ses_x', 's1', 'x', 'x')",
                [],
            )
            .is_err()
        );
    }

    /// The full-text index starts with every item and its comments, and
    /// follows each change to an item's title, body or comments.
    #[test]
    fn the_search_index_follows_the_items_and_their_comments() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        let before: Vec<Migration> = MIGRATIONS
            .iter()
            .filter(|m| m.0 < 20261117120000)
            .copied()
            .collect();
        run(&conn, &before).unwrap();
        let t = "'2026-11-01T10:00:00.000000Z'";
        conn.execute_batch(&format!(
            "INSERT INTO repos (id, name, prefix, created_at) VALUES (1, 'acme/api', 'API', {t});
             INSERT INTO work_items (id, repo_id, number, title, body, status, created_by, created_at, updated_at)
               VALUES ('a', 1, 1, 'Flickering header', 'The header jumps', 'open', 'person', {t}, {t});
             INSERT INTO work_item_comments (id, item_id, body, author, created_at)
               VALUES ('c1', 'a', 'Seen on the wombat page', 'person', {t});"
        ))
        .unwrap();
        run(&conn, MIGRATIONS).unwrap();

        let found = |words: &str| -> Vec<String> {
            conn.prepare(
                "SELECT r.item_id FROM work_item_search s
                   JOIN work_item_search_rows r ON r.row = s.rowid
                  WHERE work_item_search MATCH ?1 ORDER BY r.item_id",
            )
            .unwrap()
            .query_map([words], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
        };
        // the item from before, its comment, and its words in other forms
        assert_eq!(found("flicker"), ["a"]);
        assert_eq!(found("jumping"), ["a"]);
        assert_eq!(found("wombat"), ["a"]);

        conn.execute_batch(&format!(
            "INSERT INTO work_items (id, repo_id, number, title, body, status, created_by, created_at, updated_at)
               VALUES ('b', 1, 2, 'Slow search', '', 'open', 'person', {t}, {t});
             INSERT INTO work_item_comments (id, item_id, body, author, created_at)
               VALUES ('c2', 'b', 'Also the wombat page', 'person', {t});"
        ))
        .unwrap();
        assert_eq!(found("slow"), ["b"]);
        assert_eq!(found("wombat"), ["a", "b"]);

        conn.execute_batch(
            "UPDATE work_items SET title = 'Fast search', body = 'Ranked' WHERE id = 'b';
             UPDATE work_item_comments SET body = 'Seen on the quokka page' WHERE id = 'c1';
             DELETE FROM work_item_comments WHERE id = 'c2';",
        )
        .unwrap();
        assert!(found("slow").is_empty());
        assert_eq!(found("ranked"), ["b"]);
        assert!(found("wombat").is_empty());
        assert_eq!(found("quokka"), ["a"]);

        // a deleted item leaves the index, with its comments
        conn.execute("DELETE FROM work_items WHERE id = 'a'", [])
            .unwrap();
        assert!(found("quokka").is_empty());
        assert!(found("flicker").is_empty());
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM work_item_search_rows", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(rows, 1);
    }
}
