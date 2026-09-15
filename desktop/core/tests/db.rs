//! The database's own promises: a review ends once whichever way, status
//! is answered by SQL, and a file written before `outcomes` existed reads
//! the same afterwards.

use std::path::Path;

use chrono::{Duration, Utc};
use rusqlite::OptionalExtension;
use serde_json::{Map, json};
use wicket_core::db::{Db, Filters, SCHEMA_VERSION};
use wicket_core::review::{Decision, Review, Status, parse_datetime};

fn review(id: &str, expires_in: Option<Duration>) -> Review {
    Review {
        id: id.into(),
        plugin: "list".into(),
        plugin_version: 1,
        plugin_release: "1.0.0".into(),
        title: format!("review {id}"),
        origin: Map::new(),
        requested_by: None,
        created_at: Utc::now(),
        expires_at: expires_in.map(|d| Utc::now() + d),
        revises: None,
        summary: None,
        payload: Some(json!({})),
        decision: None,
        agent_note: None,
        withdrawn_at: None,
        withdrawn_reason: None,
        discarded_at: None,
        discarded_by: None,
        discarded_reason: None,
    }
}

fn decision() -> Decision {
    Decision {
        decided_by: "pat".into(),
        decided_at: Utc::now(),
        data: json!({"ok": true}),
    }
}

fn by_status(db: &Db, status: Status) -> Vec<String> {
    let filters = Filters {
        statuses: vec![status],
        ..Filters::default()
    };
    db.list(&filters, Utc::now())
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect()
}

#[test]
fn a_review_ends_once_whichever_way() {
    let db = Db::in_memory().unwrap();
    db.insert_review(&review("r_1", None), None).unwrap();
    assert!(
        db.insert_decision("r_1", &decision(), Some("note"))
            .unwrap()
            .is_some()
    );
    // every other ending loses to the one on record, across kinds
    assert!(
        db.insert_discard("r_1", Utc::now(), "pat", Some("late"))
            .unwrap()
            .is_none()
    );
    assert!(
        db.insert_withdrawal("r_1", Utc::now(), None)
            .unwrap()
            .is_none()
    );
    assert!(
        db.insert_decision("r_1", &decision(), None)
            .unwrap()
            .is_none()
    );
    let r = db.get_review("r_1").unwrap().unwrap();
    assert_eq!(r.status(Utc::now()), Status::Decided);
    assert_eq!(r.agent_note.as_deref(), Some("note"));
    assert!(r.discarded_at.is_none() && r.withdrawn_at.is_none());

    db.insert_review(&review("r_2", None), None).unwrap();
    assert!(
        db.insert_discard("r_2", Utc::now(), "pat", Some("no"))
            .unwrap()
            .is_some()
    );
    assert!(
        db.insert_decision("r_2", &decision(), None)
            .unwrap()
            .is_none()
    );
    let r = db.get_review("r_2").unwrap().unwrap();
    assert_eq!(r.status(Utc::now()), Status::Discarded);
    assert_eq!(r.discarded_by.as_deref(), Some("pat"));
    assert_eq!(r.discarded_reason.as_deref(), Some("no"));
    assert!(r.decision.is_none() && r.agent_note.is_none());
}

#[test]
fn status_is_answered_by_the_query_and_the_count_is_not_a_listing() {
    let db = Db::in_memory().unwrap();
    for i in 0..7 {
        db.insert_review(&review(&format!("r_{i}"), None), None)
            .unwrap();
    }
    db.insert_review(&review("r_x", Some(Duration::seconds(-5))), None)
        .unwrap();
    db.insert_review(&review("r_y", Some(Duration::hours(1))), None)
        .unwrap();
    db.insert_decision("r_0", &decision(), None).unwrap();
    db.insert_withdrawal("r_1", Utc::now(), Some("gone"))
        .unwrap();
    db.insert_discard("r_2", Utc::now(), "pat", None).unwrap();

    assert_eq!(by_status(&db, Status::Decided), vec!["r_0"]);
    assert_eq!(by_status(&db, Status::Withdrawn), vec!["r_1"]);
    assert_eq!(by_status(&db, Status::Discarded), vec!["r_2"]);
    assert_eq!(by_status(&db, Status::Expired), vec!["r_x"]);
    let mut pending = by_status(&db, Status::Pending);
    pending.sort();
    assert_eq!(pending, vec!["r_3", "r_4", "r_5", "r_6", "r_y"]);

    // several statuses at once, and the limit applied to the matches
    let filters = Filters {
        statuses: vec![Status::Withdrawn, Status::Discarded],
        limit: 1,
        ..Filters::default()
    };
    let some = db.list(&filters, Utc::now()).unwrap();
    assert_eq!(some.len(), 1);
    assert_eq!(some[0].id, "r_2", "newest first");
    assert_eq!(
        db.count(&filters, Utc::now()).unwrap(),
        2,
        "a count ignores the limit"
    );
    let pending = Filters {
        statuses: vec![Status::Pending],
        limit: 2,
        ..Filters::default()
    };
    assert_eq!(db.count(&pending, Utc::now()).unwrap(), 5);
    assert_eq!(db.list(&pending, Utc::now()).unwrap().len(), 2);
}

/// A data directory written before `outcomes`: the three tables, one row
/// each, and one review that had ended twice.
fn old_database(path: &Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE reviews (id TEXT PRIMARY KEY, plugin TEXT NOT NULL, plugin_version INTEGER NOT NULL, title TEXT NOT NULL,
           origin TEXT NOT NULL, requested_by TEXT, payload TEXT NOT NULL, summary TEXT, revises TEXT, expires_at TEXT, created_at TEXT NOT NULL);
         CREATE TABLE decisions (review_id TEXT PRIMARY KEY, decided_by TEXT NOT NULL, decided_at TEXT NOT NULL, data TEXT NOT NULL, agent_note TEXT);
         CREATE TABLE withdrawals (review_id TEXT PRIMARY KEY, withdrawn_at TEXT NOT NULL, reason TEXT);
         CREATE TABLE discards (review_id TEXT PRIMARY KEY, discarded_at TEXT NOT NULL, discarded_by TEXT NOT NULL, reason TEXT);
         CREATE TABLE events (id INTEGER PRIMARY KEY, review_id TEXT, kind TEXT NOT NULL, actor TEXT, at TEXT NOT NULL, attrs TEXT);
         INSERT INTO reviews VALUES ('r_d', 'list', 1, 'decided', '{}', NULL, '{}', NULL, NULL, NULL, '2026-09-01T10:00:00Z');
         INSERT INTO reviews VALUES ('r_w', 'list', 1, 'withdrawn', '{}', NULL, '{}', NULL, NULL, NULL, '2026-09-01T10:01:00Z');
         INSERT INTO reviews VALUES ('r_x', 'list', 1, 'discarded', '{}', NULL, '{}', NULL, NULL, NULL, '2026-09-01T10:02:00Z');
         INSERT INTO reviews VALUES ('r_p', 'list', 1, 'pending', '{}', NULL, '{}', NULL, NULL, NULL, '2026-09-01T10:03:00Z');
         INSERT INTO reviews VALUES ('r_2', 'list', 1, 'ended twice', '{}', NULL, '{}', NULL, NULL, NULL, '2026-09-01T10:04:00Z');
         INSERT INTO decisions VALUES ('r_d', 'pat', '2026-09-01T11:00:00Z', '{\"ok\":true}', 'a note');
         INSERT INTO withdrawals VALUES ('r_w', '2026-09-01T11:01:00Z', 'gone');
         INSERT INTO discards VALUES ('r_x', '2026-09-01T11:02:00Z', 'sam', 'not now');
         INSERT INTO discards VALUES ('r_2', '2026-09-01T11:03:00Z', 'sam', 'first');
         INSERT INTO decisions VALUES ('r_2', 'pat', '2026-09-01T11:03:30Z', '{\"ok\":false}', NULL);",
    )
    .unwrap();
}

#[test]
fn a_database_from_before_outcomes_reads_the_same_afterwards() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wicket.db");
    old_database(&path);

    let db = Db::open(&path).unwrap();
    let now = Utc::now();
    let get = |id: &str| db.get_review(id).unwrap().unwrap();
    let d = get("r_d");
    assert_eq!(d.status(now), Status::Decided);
    assert_eq!(d.decision.as_ref().unwrap().decided_by, "pat");
    assert_eq!(
        d.decision.as_ref().unwrap().decided_at,
        parse_datetime("2026-09-01T11:00:00Z").unwrap()
    );
    assert_eq!(d.decision.as_ref().unwrap().data, json!({"ok": true}));
    assert_eq!(d.agent_note.as_deref(), Some("a note"));
    let w = get("r_w");
    assert_eq!(w.status(now), Status::Withdrawn);
    assert_eq!(w.withdrawn_reason.as_deref(), Some("gone"));
    assert_eq!(w.withdrawn_at, parse_datetime("2026-09-01T11:01:00Z"));
    let x = get("r_x");
    assert_eq!(x.status(now), Status::Discarded);
    assert_eq!(x.discarded_by.as_deref(), Some("sam"));
    assert_eq!(x.discarded_reason.as_deref(), Some("not now"));
    assert_eq!(get("r_p").status(now), Status::Pending);
    // the review that had ended twice keeps its first ending
    let twice = get("r_2");
    assert_eq!(twice.status(now), Status::Discarded);
    assert!(twice.decision.is_none());

    // the old tables are gone, the version is stamped, and opening again is a no-op
    drop(db);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(tables.contains(&"outcomes".to_string()));
    for gone in ["decisions", "withdrawals", "discards"] {
        assert!(!tables.contains(&gone.to_string()), "{gone} is still there");
    }
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    drop(conn);
    let again = Db::open(&path).unwrap();
    assert_eq!(
        again.get_review("r_2").unwrap().unwrap().status(now),
        Status::Discarded
    );
}

#[test]
fn a_database_from_a_newer_build_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wicket.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(&format!("PRAGMA user_version = {}", SCHEMA_VERSION + 1))
        .unwrap();
    drop(conn);
    let error = Db::open(&path).unwrap_err().to_string();
    assert!(error.contains("newer than this build"), "{error}");
}

#[test]
fn plugin_directories_become_links() {
    let dir = tempfile::tempdir().unwrap();
    // two plugins in a registered directory, and a folder without a manifest
    let plugins = dir.path().join("plugins");
    for name in ["alpha", "beta"] {
        let sub = plugins.join(name);
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("index.html"), "<html></html>").unwrap();
        std::fs::write(
            sub.join("manifest.json"),
            format!("{{\"name\":\"{name}\",\"version\":2,\"payload_schema\":{{}},\"decision_schema\":{{}}}}"),
        )
        .unwrap();
    }
    std::fs::create_dir_all(plugins.join("notes")).unwrap();
    let path = dir.path().join("wicket.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE plugin_dirs (path TEXT PRIMARY KEY, added_at TEXT NOT NULL);
         INSERT INTO plugin_dirs VALUES ('{}', '2026-09-01T10:00:00Z');",
        plugins.display()
    ))
    .unwrap();
    drop(conn);

    let db = Db::open(&path).unwrap();
    let mut records = db.installed_plugins().unwrap();
    records.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(records.len(), 2);
    for (record, name) in records.iter().zip(["alpha", "beta"]) {
        assert_eq!(record.name, name);
        assert!(record.linked);
        assert_eq!(record.kind, "path");
        assert_eq!(record.version, "2.0.0");
        assert_eq!(record.major, 2);
        assert_eq!(record.path, plugins.join(name).display().to_string());
    }
    drop(db);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let gone: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'plugin_dirs'",
            [],
            |_| Ok(()),
        )
        .optional()
        .unwrap()
        .is_none();
    assert!(gone);
}

/// A file that had been through the outcomes step, as every database was
/// on the day installed_plugins arrived: the step must create its own
/// table, because the baseline never runs again for a file past it.
#[test]
fn a_step_creates_the_tables_it_adds_for_a_file_past_the_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wicket.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE reviews (id TEXT PRIMARY KEY, plugin TEXT NOT NULL, plugin_version INTEGER NOT NULL, title TEXT NOT NULL,
           origin TEXT NOT NULL, requested_by TEXT, payload TEXT NOT NULL, summary TEXT, revises TEXT, expires_at TEXT, created_at TEXT NOT NULL);
         CREATE TABLE events (id INTEGER PRIMARY KEY, review_id TEXT, kind TEXT NOT NULL, actor TEXT, at TEXT NOT NULL, attrs TEXT);
         CREATE TABLE outcomes (review_id TEXT PRIMARY KEY, kind TEXT NOT NULL, at TEXT NOT NULL, by TEXT, reason TEXT, data TEXT, agent_note TEXT);
         CREATE TABLE plugin_dirs (path TEXT PRIMARY KEY, added_at TEXT NOT NULL);
         PRAGMA user_version = 2;",
    )
    .unwrap();
    drop(conn);
    let db = Db::open(&path).unwrap();
    assert!(db.installed_plugins().unwrap().is_empty());
    drop(db);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
}
