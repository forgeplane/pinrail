//! The database's own promises: a review ends once whichever way, status
//! is answered by SQL, and migrations: a new file records what it ran, a
//! file from the numbered steps is adopted as it is, and a file from too
//! old or too new a build is refused.

use std::path::Path;

use chrono::{Duration, Utc};
use pinrail_core::db::{Db, Filters, LATEST_MIGRATION};
use pinrail_core::reviews::{Decision, Review, Status};
use serde_json::{Map, json};

fn review(id: &str, expires_in: Option<Duration>) -> Review {
    Review {
        artifacts: Vec::new(),
        artifacts_total: (0, 0),
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

/// The tables as the numbered steps left them, the day migrations moved
/// to `schema_migrations`: `plugin_release` added last by `ALTER TABLE`,
/// and `PRAGMA user_version = 4`.
const NUMBERED_STEP_4: &str = "
CREATE TABLE reviews (id TEXT PRIMARY KEY, plugin TEXT NOT NULL, plugin_version INTEGER NOT NULL, title TEXT NOT NULL,
  origin TEXT NOT NULL, requested_by TEXT, payload TEXT NOT NULL, summary TEXT, revises TEXT REFERENCES reviews(id),
  expires_at TEXT, created_at TEXT NOT NULL);
ALTER TABLE reviews ADD COLUMN plugin_release TEXT;
CREATE INDEX reviews_created ON reviews(created_at DESC);
CREATE INDEX reviews_revises ON reviews(revises);
CREATE TABLE events (id INTEGER PRIMARY KEY, review_id TEXT REFERENCES reviews(id), kind TEXT NOT NULL, actor TEXT, at TEXT NOT NULL, attrs TEXT);
CREATE INDEX events_review ON events(review_id, id);
CREATE TABLE outcomes (review_id TEXT PRIMARY KEY REFERENCES reviews(id), kind TEXT NOT NULL CHECK (kind IN ('decided', 'withdrawn', 'discarded')),
  at TEXT NOT NULL, by TEXT, reason TEXT, data TEXT, agent_note TEXT);
CREATE INDEX outcomes_kind ON outcomes(kind);
CREATE TABLE installed_plugins (name TEXT PRIMARY KEY, version TEXT NOT NULL, major INTEGER NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('path', 'git', 'release')), source TEXT NOT NULL, resolved TEXT NOT NULL,
  commit_id TEXT, asset_hash TEXT, hash TEXT, build_log TEXT, installed_at TEXT NOT NULL,
  linked INTEGER NOT NULL DEFAULT 0, path TEXT NOT NULL);
INSERT INTO reviews VALUES ('r_d', 'list', 1, 'decided', '{}', NULL, '{}', NULL, NULL, NULL, '2026-09-01T10:00:00Z', '1.0.0');
INSERT INTO reviews VALUES ('r_p', 'list', 1, 'pending', '{}', NULL, '{}', NULL, NULL, NULL, '2026-09-01T10:03:00Z', NULL);
INSERT INTO outcomes VALUES ('r_d', 'decided', '2026-09-01T11:00:00Z', 'pat', NULL, '{\"ok\":true}', 'a note');
PRAGMA user_version = 4;
";

/// Every table's columns, as `name type notnull default pk`, and every
/// index: what two files must share to be the same schema, whatever order
/// or text made them.
fn shape(path: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(path).unwrap();
    let names: Vec<(String, String)> = conn
        .prepare("SELECT type, name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut shape = Vec::new();
    for (kind, name) in names {
        shape.push(format!("{kind} {name}"));
        if kind == "table" {
            let mut stmt = conn.prepare(&format!("PRAGMA table_info({name})")).unwrap();
            let columns = stmt
                .query_map([], |r| {
                    Ok(format!(
                        "  {} {} {} {:?} {}",
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, Option<String>>(4)?,
                        r.get::<_, i64>(5)?
                    ))
                })
                .unwrap();
            for column in columns {
                shape.push(column.unwrap());
            }
        }
    }
    shape
}

fn applied(path: &Path) -> (Vec<i64>, i64) {
    let conn = rusqlite::Connection::open(path).unwrap();
    let versions = conn
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let numbered = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    (versions, numbered)
}

#[test]
fn a_new_file_records_each_migration_it_ran() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pinrail.db");
    drop(Db::open(&path).unwrap());
    let (versions, numbered) = applied(&path);
    assert_eq!(versions.last(), Some(&LATEST_MIGRATION));
    assert_eq!(numbered, 0);
    // opening again runs nothing
    drop(Db::open(&path).unwrap());
    assert_eq!(applied(&path).0, versions);
}

#[test]
fn a_file_from_the_numbered_steps_is_adopted_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.db");
    let conn = rusqlite::Connection::open(&old).unwrap();
    conn.execute_batch(NUMBERED_STEP_4).unwrap();
    drop(conn);

    let db = Db::open(&old).unwrap();
    let now = Utc::now();
    let decided = db.get_review("r_d").unwrap().unwrap();
    assert_eq!(decided.status(now), Status::Decided);
    assert_eq!(decided.plugin_release, "1.0.0");
    assert_eq!(
        db.get_review("r_p").unwrap().unwrap().status(now),
        Status::Pending
    );
    drop(db);
    let (versions, numbered) = applied(&old);
    assert_eq!(
        versions.first(),
        Some(&20260923193000),
        "the baseline is recorded, not run"
    );
    assert_eq!(numbered, 0);

    // and it is the same schema a new file gets
    let new = dir.path().join("new.db");
    drop(Db::open(&new).unwrap());
    assert_eq!(shape(&old), shape(&new));
}

#[test]
fn a_file_from_before_the_last_numbered_step_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pinrail.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE reviews (id TEXT PRIMARY KEY); PRAGMA user_version = 2;")
        .unwrap();
    drop(conn);
    let error = Db::open(&path).unwrap_err().to_string();
    assert!(
        error.contains("from before 2026-09-23 (schema step 2)"),
        "{error}"
    );
}

#[test]
fn a_file_a_newer_build_migrated_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pinrail.db");
    drop(Db::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "INSERT INTO schema_migrations VALUES (20990101000000, NULL)",
        [],
    )
    .unwrap();
    drop(conn);
    let error = Db::open(&path).unwrap_err().to_string();
    assert!(error.contains("newer Pinrail"), "{error}");
}
