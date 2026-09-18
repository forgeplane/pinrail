//! Plugin workflows through the application service, without HTTP.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use wicket_core::db::Db;
use wicket_core::events;
use wicket_core::plugins::{InstallJob, InstallOptions, PluginService, UpdateOutcome};
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

async fn finished(plugins: &PluginService, id: &str) -> InstallJob {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let job = plugins.job(id).unwrap();
            if matches!(job.status.as_str(), "done" | "failed") {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("installation did not finish")
}

#[tokio::test]
async fn inspection_and_update_jobs_work_without_http() {
    let dir = tempfile::tempdir().unwrap();
    let app = Wicket::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let sources = dir.path().join("sources");
    let source = plugin(&sources, "hello", "1.0.0");
    let source = source.to_str().unwrap();
    let db = Db::open(&app.config().db_path()).unwrap();
    let mut notices = app.reviews().bus().subscribe();

    let inspected = app
        .plugins()
        .inspect(source, InstallOptions::default())
        .await
        .unwrap();
    assert_eq!(inspected["name"], "hello");
    assert!(db.installed_plugins().unwrap().is_empty());
    assert!(!app.config().plugin_store_dir().join("hello").exists());
    assert!(notices.try_recv().is_err());

    // A cloned service shares the same jobs and notifications.
    let id = app
        .plugins()
        .clone()
        .start_install(source, InstallOptions::default());
    let installed = finished(app.plugins(), &id).await;
    assert_eq!(installed.status, "done", "{installed:?}");
    assert_eq!(installed.plugin.unwrap()["release"], "1.0.0");
    assert_eq!(notices.try_recv().unwrap().kind, events::PLUGINS_RELOADED);
    assert_eq!(
        app.plugins().check_updates("hello").await.unwrap()["state"],
        "up_to_date"
    );
    assert_eq!(
        app.plugins().start_update("hello").await.unwrap(),
        UpdateOutcome::UpToDate {
            version: "1.0.0".into()
        }
    );
    assert!(notices.try_recv().is_err());
    assert_eq!(db.events_after(0, 10).unwrap().len(), 1);

    plugin(&sources, "hello", "1.0.1");
    assert_eq!(
        app.plugins().check_updates("hello").await.unwrap()["state"],
        "available"
    );
    let UpdateOutcome::Started { job_id } = app.plugins().start_update("hello").await.unwrap()
    else {
        panic!("the changed source should start an update job");
    };
    let updated = finished(app.plugins(), &job_id).await;
    assert_eq!(updated.status, "done", "{updated:?}");
    assert_eq!(updated.plugin.unwrap()["release"], "1.0.1");
    assert_eq!(notices.try_recv().unwrap().kind, events::PLUGINS_RELOADED);
    assert!(notices.try_recv().is_err());
    assert_eq!(db.events_after(0, 10).unwrap().len(), 2);
    assert_eq!(db.installed_plugins().unwrap()[0].version, "1.0.1");
}

#[tokio::test]
async fn a_failed_build_records_its_log_without_registering_or_announcing_a_plugin() {
    let dir = tempfile::tempdir().unwrap();
    let app = Wicket::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let source = plugin(&dir.path().join("sources"), "broken", "1.0.0");
    let manifest_path = source.join("manifest.json");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest["build"] = json!({"command": "echo build-failed; exit 1"});
    std::fs::write(manifest_path, manifest.to_string()).unwrap();
    let mut notices = app.reviews().bus().subscribe();

    let id = app
        .plugins()
        .start_install(source.to_str().unwrap(), InstallOptions::default());
    let failed = finished(app.plugins(), &id).await;
    assert_eq!(failed.status, "failed");
    assert!(failed.log.contains("build-failed"), "{failed:?}");
    assert!(
        failed
            .error
            .as_deref()
            .unwrap()
            .contains("the build failed")
    );
    assert!(failed.plugin.is_none());
    assert!(matches!(
        app.plugins().versions("broken"),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        app.plugins().job("missing"),
        Err(Error::NotFound(_))
    ));
    assert!(notices.try_recv().is_err());
    let db = Db::open(&app.config().db_path()).unwrap();
    assert!(db.installed_plugins().unwrap().is_empty());
    assert!(db.events_after(0, 10).unwrap().is_empty());
}

#[tokio::test]
async fn removal_keeps_the_version_an_existing_review_needs() {
    let dir = tempfile::tempdir().unwrap();
    let app = Wicket::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let sources = dir.path().join("sources");
    let source = plugin(&sources, "hello", "1.0.0");
    let id = app
        .plugins()
        .start_install(source.to_str().unwrap(), InstallOptions::default());
    let installed = finished(app.plugins(), &id).await;
    assert_eq!(installed.status, "done", "{installed:?}");
    let review = app
        .reviews()
        .submit(
            &json!({"plugin": "hello", "title": "Keep this review"}),
            None,
        )
        .unwrap();

    plugin(&sources, "hello", "2.0.0");
    let UpdateOutcome::Started { job_id } = app.plugins().start_update("hello").await.unwrap()
    else {
        panic!("a newer major version should start an update job");
    };
    let updated = finished(app.plugins(), &job_id).await;
    assert_eq!(updated.status, "done", "{updated:?}");
    assert_eq!(
        app.plugins().versions("hello").unwrap()["versions"],
        json!([1, 2])
    );

    let removed = app.plugins().remove("hello").unwrap();
    assert_eq!(removed["entries_kept"], json!([1]));
    assert_eq!(removed["entries_removed"], json!([2]));
    let versions = app.plugins().versions("hello").unwrap();
    assert_eq!(versions["current"], Value::Null);
    assert_eq!(versions["versions"], json!([1]));
    assert_eq!(
        app.plugins().fetch_version("hello", 1).unwrap().release,
        "1.0.0"
    );
    assert!(app.plugins().fetch_version("hello", 2).is_err());
    app.reviews().decide(&review.id, &json!({}), None).unwrap();
    assert!(app.reviews().get(&review.id).unwrap().decision.is_some());
}

#[tokio::test]
async fn a_link_refuses_update_without_starting_work_or_announcing_a_change() {
    let dir = tempfile::tempdir().unwrap();
    let app = Wicket::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let sources = dir.path().join("sources");
    plugin(&sources, "hello", "1.0.0");
    app.plugins().add_dir(&sources).unwrap();
    let mut notices = app.reviews().bus().subscribe();

    assert_eq!(
        app.plugins().check_updates("hello").await.unwrap()["state"],
        "linked"
    );
    let error = app.plugins().start_update("hello").await.unwrap_err();
    assert!(error.to_string().contains("is a link"), "{error}");
    assert!(matches!(
        app.plugins().start_update("missing").await,
        Err(Error::NotFound(_))
    ));
    assert!(notices.try_recv().is_err());
    let db = Db::open(&app.config().db_path()).unwrap();
    assert_eq!(db.events_after(0, 10).unwrap().len(), 1);
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
