//! Application operations without an HTTP router or a desktop runtime.

use serde_json::{Value, json};
use wicket_core::db::Db;
use wicket_core::{Config, Error, Wicket};

#[test]
fn settings_changes_are_persisted_and_announced_without_http() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path(), 0);
    let app = Wicket::open(config.clone()).unwrap();
    let mut notices = app.reviews.bus().subscribe();

    let after = app
        .change_settings(&json!({"notifications": {"enabled": false}}))
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
    app.change_settings(&json!({"notifications": {"enabled": false}}))
        .unwrap();
    assert!(notices.try_recv().is_err());
    assert_eq!(db.events_after(0, 10).unwrap().len(), 1);

    let reopened = Wicket::open(config).unwrap();
    assert_eq!(reopened.settings.get()["notifications"]["enabled"], false);
    assert!(!dir.path().join("server.json").exists());
}

#[test]
fn external_settings_edits_are_recorded_and_announced_once() {
    let dir = tempfile::tempdir().unwrap();
    let app = Wicket::open(Config::new(dir.path(), 0)).unwrap();
    let mut notices = app.reviews.bus().subscribe();
    assert!(app.reload_settings().unwrap().is_none());

    std::fs::write(dir.path().join("settings.json"), r#"{"autostart":true}"#).unwrap();
    assert_eq!(
        app.reload_settings().unwrap(),
        Some(vec!["/autostart".into()])
    );
    assert_eq!(app.settings.get()["autostart"], true);
    let notice = notices.try_recv().unwrap();
    assert_eq!(notice.kind, "settings_changed");
    assert_eq!(notice.keys, Some(vec!["/autostart".into()]));

    assert!(app.reload_settings().unwrap().is_none());
    assert!(notices.try_recv().is_err());
    let db = Db::open(&app.config.db_path()).unwrap();
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
    let mut notices = app.reviews.bus().subscribe();

    let error = app
        .change_settings(&json!({
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
    assert_eq!(app.settings.get()["autostart"], false);
    assert_eq!(app.settings.get()["plugins"]["knobs"], Value::Null);
    assert!(!dir.path().join("settings.json").exists());
    assert!(notices.try_recv().is_err());
}
