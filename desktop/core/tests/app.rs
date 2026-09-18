//! Application operations without an HTTP router or a desktop runtime.

use serde_json::{Value, json};
use wicket_core::db::Db;
use wicket_core::{Config, Error, Wicket};

#[test]
fn event_history_hydrates_reviews_and_shared_notices_in_cursor_order() {
    let dir = tempfile::tempdir().unwrap();
    let app = Wicket::open(Config::new(dir.path(), 0)).unwrap();
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

    let notices = app.events_after(0, 10).unwrap();
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

    let page = app.events_after(notices[0].event_id, 2).unwrap();
    assert_eq!(
        page.iter().map(|n| n.event_id).collect::<Vec<_>>(),
        vec![notices[1].event_id, notices[2].event_id]
    );
    assert!(
        app.events_after(notices[3].event_id, 10)
            .unwrap()
            .is_empty()
    );
    assert!(app.events_after(0, 0).unwrap().is_empty());
}

#[test]
fn event_history_returns_storage_failures_to_the_caller() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path(), 0);
    let app = Wicket::open(config.clone()).unwrap();
    let connection = rusqlite::Connection::open(config.db_path()).unwrap();
    connection.execute_batch("DROP TABLE events").unwrap();
    assert!(matches!(app.events_after(0, 10), Err(Error::Internal(_))));
}

#[test]
fn settings_changes_are_persisted_and_announced_without_http() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path(), 0);
    let app = Wicket::open(config.clone()).unwrap();
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

    let reopened = Wicket::open(config).unwrap();
    assert_eq!(reopened.settings().get()["notifications"]["enabled"], false);
    assert!(!dir.path().join("server.json").exists());
}

#[test]
fn external_settings_edits_are_recorded_and_announced_once() {
    let dir = tempfile::tempdir().unwrap();
    let app = Wicket::open(Config::new(dir.path(), 0)).unwrap();
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

#[test]
fn invalid_plugin_settings_do_not_partially_apply_a_patch() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path(), 0);
    let plugin = config.user_plugins_dir().join("knobs");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(plugin.join("index.html"), "<html></html>").unwrap();
    std::fs::write(
        plugin.join("manifest.json"),
        json!({
            "name": "knobs", "version": 1, "title": "Knobs",
            "payload_schema": {}, "decision_schema": {},
            "settings_schema": {
                "type": "object",
                "properties": {"wrap": {"type": "boolean", "default": true}}
            }
        })
        .to_string(),
    )
    .unwrap();
    let app = Wicket::open(config).unwrap();
    let mut notices = app.events().subscribe();

    let error = app
        .settings()
        .change(&json!({
            "autostart": true,
            "plugins": {"knobs": {"wrap": "yes"}}
        }))
        .unwrap_err();
    let Error::Invalid(violations) = error else {
        panic!("expected validation errors, got {error:?}");
    };
    assert!(
        violations.iter().any(|v| v.path == "/plugins/knobs/wrap"),
        "{violations:?}"
    );
    assert_eq!(app.settings().get()["autostart"], false);
    assert_eq!(app.settings().get()["plugins"]["knobs"], Value::Null);
    assert!(!dir.path().join("settings.json").exists());
    assert!(notices.try_recv().is_err());
}
