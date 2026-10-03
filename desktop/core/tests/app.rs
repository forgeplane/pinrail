//! Application operations without an HTTP router or a desktop runtime.

use pinrail_core::db::Db;
use pinrail_core::plugins::InstallOptions;
use pinrail_core::{Config, Error, Pinrail};
use serde_json::{Value, json};

#[test]
fn event_history_hydrates_reviews_and_shared_notices_in_cursor_order() {
    let dir = tempfile::tempdir().unwrap();
    let app = Pinrail::open(Config::new(dir.path(), 0)).unwrap();
    let review = app
        .reviews()
        .submit(
            &json!({
                "plugin": "list", "title": "Review these options",
                "payload": {"groups": []}
            }),
            None,
        )
        .unwrap();
    app.settings().change(&json!({"autostart": true})).unwrap();
    app.plugins().reload().unwrap();
    let withdrawn = app
        .reviews()
        .withdraw(&review.id, Some("No longer needed"))
        .unwrap();

    let notices = app.events().after(0, 10).unwrap();
    assert_eq!(
        notices.iter().map(|n| n.kind.as_str()).collect::<Vec<_>>(),
        vec![
            "created",
            "settings_changed",
            "plugins_reloaded",
            "withdrawn"
        ]
    );
    assert!(
        notices
            .windows(2)
            .all(|pair| pair[0].event_id < pair[1].event_id)
    );
    for index in [0, 3] {
        let notice = &notices[index];
        assert_eq!(notice.review_id.as_deref(), Some(review.id.as_str()));
        assert_eq!(
            notice.review,
            Some(withdrawn.to_json(false)),
            "catch-up uses the current review"
        );
        assert!(notice.review.as_ref().unwrap().get("payload").is_none());
        assert!(notice.keys.is_none());
    }
    assert_eq!(notices[1].keys, Some(vec!["/autostart".into()]));
    for index in [1, 2] {
        assert!(notices[index].review.is_none());
        assert!(notices[index].review_id.is_none());
    }
    assert!(notices[2].keys.is_none());

    let page = app.events().after(notices[0].event_id, 2).unwrap();
    assert_eq!(
        page.iter().map(|n| n.event_id).collect::<Vec<_>>(),
        vec![notices[1].event_id, notices[2].event_id]
    );
    assert!(
        app.events()
            .after(notices[3].event_id, 10)
            .unwrap()
            .is_empty()
    );
    assert!(app.events().after(0, 0).unwrap().is_empty());
}

#[test]
fn event_history_returns_storage_failures_to_the_caller() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path(), 0);
    let app = Pinrail::open(config.clone()).unwrap();
    let connection = rusqlite::Connection::open(config.db_path()).unwrap();
    connection.execute_batch("DROP TABLE events").unwrap();
    let error = app.events().after(0, 10).unwrap_err();
    assert!(matches!(error, Error::Database(_)), "{error:?}");
    // the database's own error comes along, for whoever reports it
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn settings_changes_are_persisted_and_announced_without_http() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path(), 0);
    let app = Pinrail::open(config.clone()).unwrap();
    let mut notices = app.events().subscribe();

    let after = app
        .settings()
        .change(&json!({"notifications": {"enabled": false}}))
        .unwrap();
    assert_eq!(after["notifications"]["enabled"], false);
    let notice = notices.try_recv().unwrap();
    assert_eq!(notice.kind, "settings_changed");
    assert_eq!(notice.keys, Some(vec!["/notifications/enabled".into()]));

    let db = Db::open(&config.db_path()).unwrap();
    let events = db.events_after(0, 10).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, notice.event_id);
    assert_eq!(events[0].attrs["keys"], json!(["/notifications/enabled"]));

    // Repeating a patch neither records nor broadcasts another change.
    app.settings()
        .change(&json!({"notifications": {"enabled": false}}))
        .unwrap();
    assert!(notices.try_recv().is_err());
    assert_eq!(db.events_after(0, 10).unwrap().len(), 1);

    drop(app);
    let reopened = Pinrail::open(config).unwrap();
    assert_eq!(reopened.settings().get()["notifications"]["enabled"], false);
    assert!(!dir.path().join("server.json").exists());
}

#[test]
fn external_settings_edits_are_recorded_and_announced_once() {
    let dir = tempfile::tempdir().unwrap();
    let app = Pinrail::open(Config::new(dir.path(), 0)).unwrap();
    let mut notices = app.events().subscribe();
    assert!(app.settings().reload().unwrap().is_none());

    std::fs::write(dir.path().join("settings.json"), r#"{"autostart":true}"#).unwrap();
    assert_eq!(
        app.settings().reload().unwrap(),
        Some(vec!["/autostart".into()])
    );
    assert_eq!(app.settings().get()["autostart"], true);
    let notice = notices.try_recv().unwrap();
    assert_eq!(notice.kind, "settings_changed");
    assert_eq!(notice.keys, Some(vec!["/autostart".into()]));

    assert!(app.settings().reload().unwrap().is_none());
    assert!(notices.try_recv().is_err());
    let db = Db::open(&app.config().db_path()).unwrap();
    assert_eq!(db.events_after(0, 10).unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_plugin_settings_do_not_partially_apply_a_patch() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), 0);
    let plugin = dir.path().join("sources").join("knobs");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::create_dir_all(plugin.join("view")).unwrap();
    std::fs::write(plugin.join("view/index.html"), "<html></html>").unwrap();
    std::fs::create_dir_all(plugin.join("schemas")).unwrap();
    std::fs::write(plugin.join("schemas/payload.schema.json"), "{}").unwrap();
    std::fs::write(plugin.join("schemas/decision.schema.json"), "{}").unwrap();
    std::fs::write(
        plugin.join("manifest.json"),
        json!({
            "name": "knobs", "version": "1.0.0", "title": "Knobs",
                        "settings_schema": {
                "type": "object",
                "properties": {"wrap": {"type": "boolean", "default": true}}
            }
        })
        .to_string(),
    )
    .unwrap();
    let app = Pinrail::open(config).unwrap();
    app.plugins()
        .install(
            &plugin.display().to_string(),
            InstallOptions {
                link: true,
                ..InstallOptions::default()
            },
        )
        .await
        .unwrap();
    let mut notices = app.events().subscribe();

    let error = app
        .settings()
        .change(&json!({
            "autostart": true,
            "plugins": {"local/knobs": {"wrap": "yes"}}
        }))
        .unwrap_err();
    let Error::Invalid(violations) = error else {
        panic!("expected validation errors, got {error:?}");
    };
    assert!(
        violations
            .iter()
            .any(|v| v.path == "/plugins/local~1knobs/wrap"),
        "{violations:?}"
    );
    assert_eq!(app.settings().get()["autostart"], false);
    assert_eq!(app.settings().get()["plugins"]["knobs"], Value::Null);
    assert!(!dir.path().join("settings.json").exists());
    assert!(notices.try_recv().is_err());
}

/// The data directory holds every review, decision and file, so only its
/// owner can read it: a new one is made that way, and an existing one open
/// to others is closed.
#[cfg(unix)]
#[test]
fn the_data_directory_is_readable_by_its_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |dir: &std::path::Path| std::fs::metadata(dir).unwrap().permissions().mode() & 0o777;
    let root = tempfile::tempdir().unwrap();

    let fresh = root.path().join("fresh");
    Pinrail::open(Config::new(&fresh, 0)).unwrap();
    assert_eq!(mode(&fresh), 0o700);

    let open = root.path().join("open");
    std::fs::create_dir(&open).unwrap();
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
    Pinrail::open(Config::new(&open, 0)).unwrap();
    assert_eq!(mode(&open), 0o700);
}

#[test]
fn a_second_open_of_a_data_directory_in_use_is_refused_and_touches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path(), 0);
    let first = Pinrail::open(config.clone()).unwrap();
    // an upload the first is still receiving
    let upload = config.attachments_dir().join("tmp").join("upload-1");
    std::fs::write(&upload, b"half a file").unwrap();

    let second = Pinrail::open(config.clone());
    assert!(
        upload.exists(),
        "the second open removed the first's upload"
    );
    assert!(
        matches!(second, Err(Error::InUse(ref message)) if message.contains(&std::process::id().to_string())),
        "{:?}",
        second.as_ref().err()
    );

    // once the first is gone, the directory opens again
    drop(first);
    Pinrail::open(config).unwrap();
}

/// A stored file that no database row names, left by a stop between
/// storing an upload and recording it, is removed at the next start once
/// it is old enough that no upload can still be recording it.
#[test]
fn a_stored_file_nothing_names_is_removed_at_start() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path(), 0);
    let blobs = config.attachments_dir().join("sha256");
    drop(Pinrail::open(config.clone()).unwrap());
    let place = |sha: &str, age: std::time::Duration| {
        let path = blobs.join(&sha[..2]).join(sha);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"left over").unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(std::time::SystemTime::now() - age)
            .unwrap();
        path
    };
    let old = place(&"a".repeat(64), std::time::Duration::from_secs(2 * 3600));
    let fresh = place(&"b".repeat(64), std::time::Duration::from_secs(60));

    drop(Pinrail::open(config).unwrap());
    assert!(!old.exists(), "an old file nothing names was kept");
    assert!(fresh.exists(), "a fresh one may still be being recorded");
}
