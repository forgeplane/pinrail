//! The database's own promises: a review ends once whichever way, status
//! is answered by SQL, and migrations: a new file records what it ran, and
//! a file from a newer build is refused.

use std::path::Path;

use chrono::{Duration, Utc};
use pinrail_core::db::{Db, Filters, LATEST_MIGRATION};
use pinrail_core::reviews::{Decision, Review, Status};
use serde_json::{Map, json};

fn review(id: &str, expires_in: Option<Duration>) -> Review {
    Review {
        attachments: Vec::new(),
        attachments_total: (0, 0),
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

/// Expiry has no row of its own, so the insert itself refuses an ending
/// once the review's time is up, whatever the caller checked before.
#[test]
fn a_review_is_read_with_its_payload_and_deleted_with_it() {
    let db = Db::in_memory().unwrap();
    let mut first = review("r_1", None);
    first.payload = Some(json!({"items": [{"id": 1}]}));
    db.insert_review(&first, None).unwrap().unwrap();
    db.insert_review(&review("r_2", None), None)
        .unwrap()
        .unwrap();

    let read = db.get_review("r_1").unwrap().unwrap();
    assert_eq!(read.payload, Some(json!({"items": [{"id": 1}]})));

    assert_eq!(db.delete_reviews(&["r_1"]).unwrap(), 1);
    assert!(db.get_review("r_1").unwrap().is_none());
    assert!(db.get_review("r_2").unwrap().is_some());
}

#[test]
fn nothing_ends_a_review_after_it_expired() {
    let db = Db::in_memory().unwrap();
    db.insert_review(&review("r_1", Some(Duration::seconds(-1))), None)
        .unwrap()
        .unwrap();
    assert!(
        db.insert_decision("r_1", &decision(), None)
            .unwrap()
            .is_none()
    );
    assert!(
        db.insert_withdrawal("r_1", Utc::now(), None)
            .unwrap()
            .is_none()
    );
    assert!(
        db.insert_discard("r_1", Utc::now(), "pat", None)
            .unwrap()
            .is_none()
    );
    assert_eq!(by_status(&db, Status::Expired), vec!["r_1".to_string()]);
}

#[test]
fn a_review_ends_once_whichever_way() {
    let db = Db::in_memory().unwrap();
    db.insert_review(&review("r_1", None), None)
        .unwrap()
        .unwrap();
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

    db.insert_review(&review("r_2", None), None)
        .unwrap()
        .unwrap();
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
            .unwrap()
            .unwrap();
    }
    db.insert_review(&review("r_x", Some(Duration::seconds(-5))), None)
        .unwrap()
        .unwrap();
    db.insert_review(&review("r_y", Some(Duration::hours(1))), None)
        .unwrap()
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

fn applied(path: &Path) -> Vec<i64> {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.prepare("SELECT version FROM schema_migrations ORDER BY version")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn a_new_file_records_each_migration_it_ran() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pinrail.db");
    drop(Db::open(&path).unwrap());
    let versions = applied(&path);
    assert_eq!(versions.last(), Some(&LATEST_MIGRATION));
    // opening again runs nothing
    drop(Db::open(&path).unwrap());
    assert_eq!(applied(&path), versions);
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
