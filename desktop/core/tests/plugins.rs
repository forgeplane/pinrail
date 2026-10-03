//! Plugin workflows through the application service, without HTTP.

use std::path::{Path, PathBuf};

use pinrail_core::db::Db;
use pinrail_core::events;
use pinrail_core::plugins::InstallOptions;
use pinrail_core::{Config, Error, Pinrail};
use serde_json::{Value, json};

fn plugin(root: &Path, name: &str, version: &str) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(dir.join("view")).unwrap();
    std::fs::write(dir.join("view/index.html"), "<html>plugin</html>").unwrap();
    std::fs::create_dir_all(dir.join("schemas")).unwrap();
    std::fs::write(dir.join("schemas/payload.schema.json"), "{}").unwrap();
    std::fs::write(dir.join("schemas/decision.schema.json"), "{}").unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        json!({
            "name": name, "version": version, "title": name,
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

/// Installs one plugin folder as a link: the plugin's row, or why not.
async fn link(app: &Pinrail, dir: &Path) -> Result<Value, Error> {
    app.plugins()
        .install(&dir.display().to_string(), InstallOptions { link: true })
        .await
}

/// Installs a copy of one plugin folder into the store.
async fn copy(app: &Pinrail, dir: &Path) -> Result<Value, Error> {
    app.plugins()
        .install(&dir.display().to_string(), InstallOptions::default())
        .await
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
    copy(&app, &plugin(&sources.join("1.2.0"), "hello", "1.2.0"))
        .await
        .unwrap();
    let before = listed(&app, "hello");

    // a release whose view is not built, and one whose schema is not JSON
    let unbuilt = plugin(&sources.join("1.3.0"), "hello", "1.3.0");
    std::fs::remove_file(unbuilt.join("view/index.html")).unwrap();
    let broken = plugin(&sources.join("1.4.0"), "hello", "1.4.0");
    std::fs::write(broken.join("schemas/payload.schema.json"), "{").unwrap();
    for source in [unbuilt, broken] {
        let refused = copy(&app, &source).await;
        assert!(refused.is_err(), "{refused:?}");
        app.plugins().reload().unwrap();
        let hello = listed(&app, "hello");
        assert_eq!(hello, before, "{}", source.display());
        assert_eq!(hello["version"], "1.2.0");
        assert_eq!(hello["install"]["modified"], false);
    }
}

#[tokio::test]
async fn an_inspection_and_installs_work_without_http() {
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
    assert!(db.install("hello").unwrap().is_none());
    assert!(notices.try_recv().is_err());

    // A cloned service shares the same notifications.
    let row = app
        .plugins()
        .clone()
        .install(source, InstallOptions::default())
        .await
        .unwrap();
    assert_eq!(row["name"], "hello");
    assert_eq!(row["version"], "1.0.0");
    assert_eq!(notices.try_recv().unwrap().kind, events::PLUGINS_RELOADED);
    assert!(notices.try_recv().is_err());
    assert_eq!(db.events_after(0, 10).unwrap().len(), 1);

    // installing again from the changed folder upgrades it
    plugin(&sources, "hello", "1.0.1");
    let updated = app
        .plugins()
        .install(source, InstallOptions::default())
        .await
        .unwrap();
    assert_eq!(updated["version"], "1.0.1");
    assert_eq!(notices.try_recv().unwrap().kind, events::PLUGINS_RELOADED);
    assert!(notices.try_recv().is_err());
    assert_eq!(db.events_after(0, 10).unwrap().len(), 2);
    let install = db.install("hello").unwrap().unwrap();
    let bundle = install.bundle.unwrap();
    assert_eq!(db.bundle(&bundle).unwrap().unwrap().version, "1.0.1");
}

/// A review keeps the release it was submitted to: through an install of
/// a new major and the plugin's removal, it renders and decides with it.
#[tokio::test]
async fn a_review_keeps_its_release_through_updates_and_removal() {
    let dir = tempfile::tempdir().unwrap();
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let sources = dir.path().join("sources");
    let source = plugin(&sources, "hello", "1.0.0");
    copy(&app, &source).await.unwrap();
    let review = app
        .reviews()
        .submit(
            &json!({"plugin": "hello", "title": "Keep this review"}),
            None,
        )
        .unwrap();
    assert_eq!(review.plugin_version, "1.0.0");

    plugin(&sources, "hello", "2.0.0");
    copy(&app, &source).await.unwrap();

    let removed = app.plugins().remove("hello").unwrap();
    assert_eq!(removed["removed"], "hello");
    assert_eq!(removed["version"], "2.0.0");
    let kept = app
        .plugins()
        .fetch_review("hello", review.plugin_bundle.as_deref())
        .unwrap();
    assert_eq!(kept.version, "1.0.0");
    app.reviews().decide(&review.id, &json!({}), None).unwrap();
    assert!(app.reviews().get(&review.id).unwrap().decision.is_some());
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

    link(&app, &linked).await.unwrap();
    let added = notices.try_recv().unwrap();
    assert_eq!(added.kind, events::PLUGINS_RELOADED);
    assert!(added.review_id.is_none());
    assert!(db.install("hello").unwrap().unwrap().linked());

    // Listing combines each plugin's defaults with the current saved settings.
    let listed = app.plugins().listing(&json!({"hello": {"wrap": false}}));
    let hello = listed["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "hello")
        .unwrap();
    assert_eq!(hello["settings"], json!({"wrap": false}));
    assert_eq!(hello["install"]["link"], true);

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
        linked.join("view/index.html").exists(),
        "a linked source is kept"
    );
    assert!(db.install("hello").unwrap().is_none());
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
        app.plugins().describe(Some("hello")),
        Err(Error::NotFound(_))
    ));
}

/// A plugin linked under the name of one the app carries takes its place,
/// keeps it across a restart, and the app's own comes back once it is
/// removed.
#[tokio::test]
async fn a_plugin_of_an_official_name_takes_its_place() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), 0);
    let app = Pinrail::open(config.clone()).unwrap();
    let official = app
        .reviews()
        .submit(
            &json!({"plugin": "list", "title": "Official", "payload": {"groups": []}}),
            None,
        )
        .unwrap();
    assert_eq!(official.plugin_version, "1.0.0");

    let mine = plugin(&dir.path().join("sources"), "list", "2.0.0");
    let row = link(&app, &mine).await.unwrap();
    assert_eq!(row["name"], "list");
    assert_eq!(row["install"]["source_kind"], "folder");
    assert_eq!(row["install"]["link"], true);
    assert_eq!(row["replaced_version"], "1.0.0");
    let local = app
        .reviews()
        .submit(&json!({"plugin": "list", "title": "Mine"}), None)
        .unwrap();
    assert_eq!(
        (local.plugin.as_str(), local.plugin_version.as_str()),
        ("list", "2.0.0")
    );

    // the start that stores the app's own leaves it in its place
    drop(app);
    let reopened = Pinrail::open(config.clone()).unwrap();
    let described = reopened.plugins().describe(Some("list")).unwrap();
    assert_eq!(described["plugins"][0]["version"], "2.0.0");

    reopened.plugins().remove("list").unwrap();
    assert!(matches!(
        reopened.plugins().describe(Some("list")),
        Err(Error::NotFound(_))
    ));
    // the review made with the app's copy renders with it
    let kept = reopened
        .plugins()
        .fetch_review("list", official.plugin_bundle.as_deref())
        .unwrap();
    assert_eq!(kept.version, "1.0.0");
    drop(reopened);
    let again = Pinrail::open(config).unwrap();
    let described = again.plugins().describe(Some("list")).unwrap();
    assert_eq!(described["plugins"][0]["version"], "1.0.0");
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
        let view = copy.path().join("view/index.html");
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
            plugin.sample_errors.is_empty(),
            "{}: {:?}",
            plugin.name,
            plugin.sample_errors
        );
        assert!(plugin.example().is_some(), "{} has an example", plugin.name);
        let sample = plugin
            .sample(None)
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
    link(&app, &model).await.unwrap();
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
    link(&app, &plugin(&sources, "bare", "1.0.0"))
        .await
        .unwrap();
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

/// A linked plugin follows its folder: a change to its manifest or a schema
/// applies at the next look, without a reload, a broken manifest shows on
/// its row until it is repaired, and the folder can go and come back.
#[tokio::test]
async fn a_linked_plugin_follows_its_folder_without_a_reload() {
    let dir = tempfile::tempdir().unwrap();
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let folder = plugin(&dir.path().join("sources"), "hello", "1.0.0");
    link(&app, &folder).await.unwrap();
    let mut notices = app.events().subscribe();
    assert!(
        !app.plugins().reload_if_links_changed().unwrap(),
        "nothing changed"
    );
    assert!(notices.try_recv().is_err());

    let manifest = folder.join("manifest.json");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        text.replace("\"title\":\"hello\"", "\"title\":\"Hello again\""),
    )
    .unwrap();
    assert!(app.plugins().reload_if_links_changed().unwrap());
    assert_eq!(listed(&app, "hello")["title"], "Hello again");
    assert_eq!(notices.try_recv().unwrap().kind, events::PLUGINS_RELOADED);

    // a schema the next submission is checked against
    std::fs::write(
        folder.join("schemas/payload.schema.json"),
        json!({"type": "object", "required": ["message"]}).to_string(),
    )
    .unwrap();
    assert!(app.plugins().reload_if_links_changed().unwrap());
    let refused = app
        .reviews()
        .submit(&json!({"plugin": "hello", "title": "No message"}), None);
    assert!(refused.is_err());

    // broken, then repaired
    std::fs::write(&manifest, "{ not json").unwrap();
    assert!(app.plugins().reload_if_links_changed().unwrap());
    assert!(listed(&app, "hello")["error"].is_string());
    std::fs::write(&manifest, &text).unwrap();
    assert!(app.plugins().reload_if_links_changed().unwrap());
    assert_eq!(listed(&app, "hello")["error"], Value::Null);

    // the folder goes, and comes back
    let moved = dir.path().join("aside");
    std::fs::rename(&folder, &moved).unwrap();
    assert!(app.plugins().reload_if_links_changed().unwrap());
    assert!(listed(&app, "hello")["error"].is_string());
    std::fs::rename(&moved, &folder).unwrap();
    assert!(app.plugins().reload_if_links_changed().unwrap());
    assert_eq!(listed(&app, "hello")["usable"], true);
}
