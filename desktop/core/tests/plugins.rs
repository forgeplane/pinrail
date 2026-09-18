//! Plugin workflows through the application service, without HTTP.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use wicket_core::db::Db;
use wicket_core::events;
use wicket_core::{Config, Error, Wicket};

fn plugin(root: &Path, name: &str, version: &str) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("index.html"), "<html>plugin</html>").unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        json!({
            "name": name, "version": version, "title": name,
            "payload_schema": {}, "decision_schema": {},
            "settings_schema": {
                "type": "object",
                "properties": {"wrap": {"type": "boolean", "default": true}}
            }
        })
        .to_string(),
    )
    .unwrap();
    dir
}

#[test]
fn directory_registration_reload_and_removal_record_and_announce_changes() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), 0);
    let sources = dir.path().join("sources");
    let linked = plugin(&sources, "hello", "1.0.0");
    let app = Wicket::open(config.clone()).unwrap();
    let mut notices = app.reviews().bus().subscribe();
    let db = Db::open(&config.db_path()).unwrap();

    assert_eq!(app.plugins().add_dir(&sources).unwrap(), 2);
    let added = notices.try_recv().unwrap();
    assert_eq!(added.kind, events::PLUGINS_RELOADED);
    assert!(added.review_id.is_none());
    assert_eq!(db.installed_plugins().unwrap()[0].name, "hello");

    // Listing combines each plugin's defaults with the current saved settings.
    let listed = app.plugins().listing(&json!({"hello": {"wrap": false}}));
    let hello = listed["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "hello")
        .unwrap();
    assert_eq!(hello["settings"], json!({"wrap": false}));
    assert_eq!(hello["install"]["linked"], true);

    assert_eq!(app.plugins().reload().unwrap(), 2);
    let reloaded = notices.try_recv().unwrap();
    assert_eq!(reloaded.kind, events::PLUGINS_RELOADED);
    let removed = app.plugins().remove("hello").unwrap();
    assert_eq!(removed["removed"], "hello");
    assert!(
        linked.join("index.html").exists(),
        "a linked source is kept"
    );
    assert!(db.installed_plugins().unwrap().is_empty());
    let removed = notices.try_recv().unwrap();
    assert_eq!(removed.kind, events::PLUGINS_RELOADED);
    let recorded = db.events_after(0, 10).unwrap();
    assert_eq!(
        recorded.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![added.event_id, reloaded.event_id, removed.event_id]
    );
    assert!(recorded.iter().all(|e| e.kind == events::PLUGINS_RELOADED));
    assert!(notices.try_recv().is_err());
    assert!(matches!(
        app.plugins().versions("hello"),
        Err(Error::NotFound(_))
    ));
    assert!(app.plugins().fetch_version("list", 1).is_ok());
}

#[test]
fn a_directory_that_shadows_a_builtin_keeps_the_registry_and_database_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), 0);
    let sources = dir.path().join("sources");
    plugin(&sources, "hello", "1.0.0");
    let app = Wicket::open(config.clone()).unwrap();
    app.plugins().add_dir(&sources).unwrap();
    let mut notices = app.reviews().bus().subscribe();
    let before = app.plugins().listing(&Value::Null);

    let duplicates = dir.path().join("duplicates");
    plugin(&duplicates, "list", "2.0.0");
    plugin(&duplicates, "another", "1.0.0");
    let error = app.plugins().add_dir(&duplicates).unwrap_err();
    assert!(matches!(error, Error::Invalid(_)));
    assert_eq!(app.plugins().listing(&Value::Null), before);
    assert!(notices.try_recv().is_err());
    let db = Db::open(&config.db_path()).unwrap();
    assert_eq!(db.installed_plugins().unwrap().len(), 1);
    assert_eq!(db.events_after(0, 10).unwrap().len(), 1);

    let reopened = Wicket::open(config).unwrap();
    assert_eq!(reopened.plugins().versions("hello").unwrap()["current"], 1);
    assert!(matches!(
        reopened.plugins().versions("another"),
        Err(Error::NotFound(_))
    ));
}
