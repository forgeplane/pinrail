//! Plugin workflows through the application service, without HTTP.

use std::path::{Path, PathBuf};
use std::time::Duration;

use pinrail_core::db::Db;
use pinrail_core::events;
use pinrail_core::plugins::{InstallJob, InstallOptions, PluginService, UpdateOutcome};
use pinrail_core::{Config, Error, Pinrail};
use serde_json::{Value, json};

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

/// Installs one plugin folder as a link, the only way a plugin arrives, and
/// waits for the job.
async fn link(app: &Pinrail, dir: &Path) -> InstallJob {
    let id = app.plugins().start_install(
        &dir.display().to_string(),
        InstallOptions {
            link: true,
            force: false,
            reference: None,
            path: None,
        },
    );
    finished(app.plugins(), &id).await
}

/// Installs a copy of one plugin folder into the store, and waits for the job.
async fn copy(app: &Pinrail, dir: &Path) -> InstallJob {
    let id = app.plugins().start_install(
        &dir.display().to_string(),
        InstallOptions {
            link: false,
            force: false,
            reference: None,
            path: None,
        },
    );
    finished(app.plugins(), &id).await
}

/// The listing's entry for one plugin.
fn listed(app: &Pinrail, name: &str) -> Value {
    app.plugins().listing(&Value::Null)["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .cloned()
        .unwrap_or(Value::Null)
}

#[tokio::test]
async fn a_refused_install_leaves_the_installed_plugin_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), 0);
    let app = Pinrail::open(config.clone()).unwrap();
    let sources = dir.path().join("sources");
    assert_eq!(
        copy(&app, &plugin(&sources.join("1.0.0"), "hello", "1.0.0"))
            .await
            .status,
        "done"
    );

    // the same line and a new one, each refused when the registry reloads:
    // a folder among the built-in plugins claims the name too
    for version in ["1.1.0", "2.0.0"] {
        let stray = plugin(&config.builtin_plugins_dir(), "hello", "9.0.0");
        let job = copy(&app, &plugin(&sources.join(version), "hello", version)).await;
        assert_eq!(job.status, "failed", "{version}: {job:?}");
        std::fs::remove_dir_all(stray).unwrap();

        app.plugins().reload().unwrap();
        let hello = listed(&app, "hello");
        assert_eq!(hello["release"], "1.0.0", "{version}: {hello}");
        assert_eq!(hello["error"], Value::Null, "{version}: {hello}");
        assert_eq!(hello["install"]["modified"], false, "{version}: {hello}");
    }
}

#[tokio::test]
async fn inspection_and_update_jobs_work_without_http() {
    let dir = tempfile::tempdir().unwrap();
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let sources = dir.path().join("sources");
    let source = plugin(&sources, "hello", "1.0.0");
    let source = source.to_str().unwrap();
    let db = Db::open(&app.config().db_path()).unwrap();
    let mut notices = app.events().subscribe();

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
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let source = plugin(&dir.path().join("sources"), "broken", "1.0.0");
    let manifest_path = source.join("manifest.json");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest["build"] = json!({"command": "echo build-failed; exit 1"});
    std::fs::write(manifest_path, manifest.to_string()).unwrap();
    let mut notices = app.events().subscribe();

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
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();
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
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let sources = dir.path().join("sources");
    link(&app, &plugin(&sources, "hello", "1.0.0")).await;
    let mut notices = app.events().subscribe();

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

#[tokio::test]
async fn linking_reload_and_removal_record_and_announce_changes() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), 0);
    let sources = dir.path().join("sources");
    let linked = plugin(&sources, "hello", "1.0.0");
    let app = Pinrail::open(config.clone()).unwrap();
    let mut notices = app.events().subscribe();
    let db = Db::open(&config.db_path()).unwrap();

    assert_eq!(link(&app, &linked).await.status, "done");
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

    assert_eq!(
        app.plugins().reload().unwrap(),
        3,
        "hello and the built-in ones"
    );
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

#[tokio::test]
async fn a_plugin_that_takes_a_builtin_name_leaves_the_registry_and_database_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), 0);
    let sources = dir.path().join("sources");
    let app = Pinrail::open(config.clone()).unwrap();
    link(&app, &plugin(&sources, "hello", "1.0.0")).await;
    let mut notices = app.events().subscribe();
    let before = app.plugins().listing(&Value::Null);

    let duplicates = dir.path().join("duplicates");
    let clash = plugin(&duplicates, "list", "2.0.0");
    let job = link(&app, &clash).await;
    assert_eq!(job.status, "failed");
    assert_eq!(app.plugins().listing(&Value::Null), before);
    assert!(notices.try_recv().is_err());
    let db = Db::open(&config.db_path()).unwrap();
    assert!(
        job.error
            .as_deref()
            .unwrap_or_default()
            .contains("list ships with Pinrail and cannot be installed over"),
        "{:?}",
        job.error
    );
    assert_eq!(
        db.installed_plugins().unwrap().len(),
        1,
        "no record is left"
    );
    assert_eq!(db.events_after(0, 10).unwrap().len(), 1);

    drop(app);
    let reopened = Pinrail::open(config).unwrap();
    assert_eq!(reopened.plugins().versions("hello").unwrap()["current"], 1);
    assert!(matches!(
        reopened.plugins().versions("another"),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn every_shipped_plugin_says_when_to_use_it_and_ships_an_example_and_a_sample() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    for entry in std::fs::read_dir(&root).unwrap().flatten() {
        if !entry.path().join("manifest.json").is_file() {
            continue;
        }
        // a view built from sources is not in a fresh checkout: stand in for it
        let copy = tempfile::tempdir().unwrap();
        copy_without_node_modules(&entry.path(), copy.path());
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(copy.path().join("manifest.json")).unwrap(),
        )
        .unwrap();
        let view = copy
            .path()
            .join(manifest["entry"].as_str().unwrap_or("index.html"));
        if !view.is_file() {
            std::fs::create_dir_all(view.parent().unwrap()).unwrap();
            std::fs::write(&view, "").unwrap();
        }
        let plugin = pinrail_core::plugins::Plugin::load(copy.path());
        assert!(plugin.usable(), "{}: {:?}", plugin.name, plugin.error);
        assert!(
            plugin.use_when.is_some(),
            "{} says when to use it",
            plugin.name
        );
        assert!(
            plugin.example.is_some(),
            "{}: {:?}",
            plugin.name,
            plugin.example_error
        );
        assert_eq!(plugin.sample_error, None, "{}", plugin.name);
        let sample = plugin
            .sample
            .as_ref()
            .unwrap_or_else(|| panic!("{} has no sample", plugin.name));
        assert!(!sample.title.is_empty(), "{}", plugin.name);
    }
}

fn copy_without_node_modules(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.file_name() == "node_modules" {
            continue;
        }
        if entry.path().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy_without_node_modules(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[tokio::test]
async fn a_sample_is_sent_as_a_review_with_its_files() {
    let dir = tempfile::tempdir().unwrap();
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();

    // a built-in plugin's sample, the title given
    let review = app
        .send_sample("list", &json!({ "title": "Try Pinrail" }))
        .unwrap();
    assert_eq!(review.plugin, "list");
    assert_eq!(review.title, "Try Pinrail");
    assert_eq!(review.requested_by.as_deref(), Some("sample"));

    // a linked plugin's sample, with the files it names stored
    let model = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/model");
    assert_eq!(link(&app, &model).await.status, "done");
    let review = app.send_sample("model", &json!({})).unwrap();
    assert_eq!(review.title, "Halden desk lamp");
    let mut names: Vec<&str> = review.attachments.iter().map(|a| a.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["arc.glb", "column.glb", "pivot.glb", "tripod.glb"]);
    for attachment in &review.attachments {
        assert!(app.attachments().path(&attachment.sha256).is_file());
    }

    // one without a sample, and none at all
    let sources = dir.path().join("sources");
    assert_eq!(
        link(&app, &plugin(&sources, "bare", "1.0.0")).await.status,
        "done"
    );
    let message = |e: Error| {
        e.to_json()["violations"][0]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    assert_eq!(
        message(app.send_sample("bare", &json!({})).unwrap_err()),
        "bare has no sample"
    );
    assert_eq!(
        message(app.send_sample("nope", &json!({})).unwrap_err()),
        "no usable plugin is named nope"
    );
}
