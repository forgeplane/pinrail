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
    let bundle = install.bundle;
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
    app.reviews()
        .decide(&review.id, &json!({}), None, None)
        .unwrap();
    assert!(app.reviews().get(&review.id).unwrap().decision.is_some());
}

#[tokio::test]
async fn linking_and_removal_record_and_announce_changes() {
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
        vec![added.event_id, removed.event_id]
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
/// applies the next time the plugin is used, without a reload, a broken manifest shows on
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
    // noticed by the check, stored when the plugin is next used
    app.plugins().describe(None).unwrap();
    assert_eq!(listed(&app, "hello")["title"], "Hello again");
    assert_eq!(notices.try_recv().unwrap().kind, events::PLUGINS_RELOADED);

    // a schema the next submission is checked against
    std::fs::write(
        folder.join("schemas/payload.schema.json"),
        json!({"type": "object", "required": ["message"]}).to_string(),
    )
    .unwrap();
    assert!(app.plugins().reload_if_links_changed().unwrap());
    // noticed by the check, stored when the plugin is next used
    app.plugins().describe(None).unwrap();
    let refused = app
        .reviews()
        .submit(&json!({"plugin": "hello", "title": "No message"}), None);
    assert!(refused.is_err());

    // broken, then repaired
    std::fs::write(&manifest, "{ not json").unwrap();
    assert!(app.plugins().reload_if_links_changed().unwrap());
    // noticed by the check, stored when the plugin is next used
    app.plugins().describe(None).unwrap();
    assert!(listed(&app, "hello")["error"].is_string());
    std::fs::write(&manifest, &text).unwrap();
    assert!(app.plugins().reload_if_links_changed().unwrap());
    // noticed by the check, stored when the plugin is next used
    app.plugins().describe(None).unwrap();
    assert_eq!(listed(&app, "hello")["error"], Value::Null);

    // the folder goes, and comes back
    let moved = dir.path().join("aside");
    std::fs::rename(&folder, &moved).unwrap();
    assert!(app.plugins().reload_if_links_changed().unwrap());
    // noticed by the check, stored when the plugin is next used
    app.plugins().describe(None).unwrap();
    assert!(listed(&app, "hello")["error"].is_string());
    std::fs::rename(&moved, &folder).unwrap();
    assert!(app.plugins().reload_if_links_changed().unwrap());
    // noticed by the check, stored when the plugin is next used
    app.plugins().describe(None).unwrap();
    assert_eq!(listed(&app, "hello")["usable"], true);
}

/// The bundles stored so far, by hash.
fn stored(app: &Pinrail) -> Vec<String> {
    let mut hashes: Vec<String> = std::fs::read_dir(app.config().plugin_bundles_dir())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    hashes.sort();
    hashes
}

/// A linked plugin is captured when it is used: a submission after the
/// folder changed is checked against the folder as it is now, and records
/// a bundle that holds it, with no reload in between.
#[tokio::test]
async fn a_linked_plugin_is_captured_when_it_is_used() {
    let dir = tempfile::tempdir().unwrap();
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let folder = plugin(&dir.path().join("sources"), "hello", "1.0.0");
    let row = link(&app, &folder).await.unwrap();
    // a link has a bundle of its own, from the start
    let first = row["install"]["bundle"].as_str().unwrap().to_string();
    let before = app
        .reviews()
        .submit(&json!({"plugin": "hello", "title": "Before"}), None)
        .unwrap();
    assert_eq!(before.plugin_bundle.as_deref(), Some(first.as_str()));

    // a payload only the new schema takes
    std::fs::write(
        folder.join("schemas/payload.schema.json"),
        json!({"type": "object", "required": ["note"], "properties": {"note": {"type": "string"}}})
            .to_string(),
    )
    .unwrap();
    let after = app
        .reviews()
        .submit(
            &json!({"plugin": "hello", "title": "After", "payload": {"note": "hi"}}),
            None,
        )
        .unwrap();
    let second = after.plugin_bundle.clone().unwrap();
    assert_ne!(second, first);
    let schema = std::fs::read_to_string(
        app.config()
            .plugin_bundles_dir()
            .join(&second)
            .join("schemas/payload.schema.json"),
    )
    .unwrap();
    assert!(schema.contains("note"), "{schema}");
    assert_eq!(listed(&app, "hello")["install"]["bundle"], second);
    // the earlier review keeps what it was submitted to
    assert_eq!(
        app.reviews()
            .get(&before.id)
            .unwrap()
            .plugin_bundle
            .as_deref(),
        Some(first.as_str())
    );
}

/// A folder that has not changed is stored once, however often its plugin
/// is used.
#[tokio::test]
async fn an_unchanged_linked_folder_is_stored_once() {
    let dir = tempfile::tempdir().unwrap();
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let folder = plugin(&dir.path().join("sources"), "hello", "1.0.0");
    link(&app, &folder).await.unwrap();
    let bundles = stored(&app);
    for i in 0..5 {
        app.plugins().describe(Some("hello")).unwrap();
        app.reviews()
            .submit(
                &json!({"plugin": "hello", "title": format!("Round {i}")}),
                None,
            )
            .unwrap();
    }
    assert_eq!(stored(&app), bundles);
}

/// A linked folder that breaks: describing and submitting are refused with
/// its problem, the row shows it, and the installation keeps its last good
/// bundle until the folder is repaired.
#[tokio::test]
async fn a_broken_linked_folder_is_refused_until_it_is_repaired() {
    let dir = tempfile::tempdir().unwrap();
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let folder = plugin(&dir.path().join("sources"), "hello", "1.0.0");
    let good = link(&app, &folder).await.unwrap()["install"]["bundle"].clone();

    let manifest = folder.join("manifest.json");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, "{ not json").unwrap();
    let described = app
        .plugins()
        .describe(Some("hello"))
        .unwrap_err()
        .to_string();
    assert!(described.contains("manifest.json"), "{described}");
    let refused = app
        .reviews()
        .submit(&json!({"plugin": "hello", "title": "Broken"}), None)
        .unwrap_err()
        .to_string();
    assert!(refused.contains("manifest.json"), "{refused}");
    let row = listed(&app, "hello");
    assert!(
        row["error"]
            .as_str()
            .unwrap_or_default()
            .contains("manifest.json"),
        "{row}"
    );
    assert_eq!(row["install"]["bundle"], good);

    std::fs::write(&manifest, &text).unwrap();
    app.reviews()
        .submit(&json!({"plugin": "hello", "title": "Repaired"}), None)
        .unwrap();
    assert_eq!(listed(&app, "hello")["error"], Value::Null);
}

/// A capture that nothing refers to any more goes with the sweep.
#[tokio::test]
async fn a_capture_nothing_refers_to_is_swept() {
    let dir = tempfile::tempdir().unwrap();
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let folder = plugin(&dir.path().join("sources"), "hello", "1.0.0");
    let first = link(&app, &folder).await.unwrap()["install"]["bundle"]
        .as_str()
        .unwrap()
        .to_string();
    std::fs::write(folder.join("view/index.html"), "<html>second</html>").unwrap();
    app.plugins().describe(Some("hello")).unwrap();
    let second = listed(&app, "hello")["install"]["bundle"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(second, first);

    let later = chrono::Utc::now() + chrono::Duration::hours(2);
    app.bundles().sweep(later).unwrap();
    let left = stored(&app);
    assert!(!left.contains(&first), "the first capture was kept");
    assert!(left.contains(&second));
}

/// Writes a decision schema that takes `{ok: bool}` and requires `verdict`.
fn require_verdict(folder: &Path) {
    std::fs::write(
        folder.join("schemas/decision.schema.json"),
        json!({"type": "object", "required": ["verdict"], "properties": {"verdict": {"type": "string"}}})
            .to_string(),
    )
    .unwrap();
}

/// A pending review of a linked plugin follows the folder: opened after the
/// folder changed, it moves to the folder as it is, and is decided by that
/// version; once the link and the folder are gone, it renders and checks
/// with what it was decided with.
#[tokio::test]
async fn a_pending_review_moves_to_the_folder_when_it_is_opened() {
    let dir = tempfile::tempdir().unwrap();
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let folder = plugin(&dir.path().join("sources"), "hello", "1.0.0");
    link(&app, &folder).await.unwrap();
    let review = app
        .reviews()
        .submit(&json!({"plugin": "hello", "title": "Follow"}), None)
        .unwrap();
    let first = review.plugin_bundle.clone().unwrap();
    let mut notices = app.events().subscribe();

    std::fs::write(folder.join("view/index.html"), "<html>second</html>").unwrap();
    require_verdict(&folder);
    let opened = app.reviews().open(&review.id).unwrap();
    assert!(opened.refused.is_none(), "{:?}", opened.refused);
    let second = opened.review.plugin_bundle.clone().unwrap();
    assert_ne!(second, first);
    assert_eq!(
        opened.plugin.describe()["decision_schema"]["required"],
        json!(["verdict"])
    );
    // the folder stored as it is, then the review moved to it
    assert_eq!(notices.try_recv().unwrap().kind, events::PLUGINS_RELOADED);
    let moved = notices.try_recv().unwrap();
    assert_eq!(moved.kind, events::PLUGIN_CHANGED);
    assert_eq!(moved.review_id.as_deref(), Some(review.id.as_str()));
    // opened again with nothing changed, it stays
    let again = app.reviews().open(&review.id).unwrap();
    assert_eq!(again.review.plugin_bundle.as_deref(), Some(second.as_str()));

    // the old schema's decision is refused, the new one's taken
    assert!(
        app.reviews()
            .decide(&review.id, &json!({"ok": true}), None, Some(&second))
            .is_err()
    );
    app.reviews()
        .decide(&review.id, &json!({"verdict": "yes"}), None, Some(&second))
        .unwrap();

    app.plugins().remove("hello").unwrap();
    std::fs::remove_dir_all(&folder).unwrap();
    let decided = app.reviews().get(&review.id).unwrap();
    assert_eq!(decided.plugin_bundle.as_deref(), Some(second.as_str()));
    let view = std::fs::read_to_string(
        app.config()
            .plugin_bundles_dir()
            .join(&second)
            .join("view/index.html"),
    )
    .unwrap();
    assert_eq!(view, "<html>second</html>");
    let rendered = app.reviews().open(&review.id).unwrap();
    assert_eq!(
        rendered.review.plugin_bundle.as_deref(),
        Some(second.as_str())
    );
}

/// A hand-over records the version that was shown: the folder may change
/// again after the review was opened, and the decision is still checked
/// by, and recorded with, the version the person saw. A hand-over from a
/// view of another version is a conflict.
#[tokio::test]
async fn a_decision_is_recorded_with_the_version_that_was_shown() {
    let dir = tempfile::tempdir().unwrap();
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let folder = plugin(&dir.path().join("sources"), "hello", "1.0.0");
    link(&app, &folder).await.unwrap();
    let review = app
        .reviews()
        .submit(&json!({"plugin": "hello", "title": "Shown"}), None)
        .unwrap();
    let shown = app
        .reviews()
        .open(&review.id)
        .unwrap()
        .review
        .plugin_bundle
        .unwrap();

    // the folder changes and is captured, but the review is not opened again
    require_verdict(&folder);
    app.plugins().describe(Some("hello")).unwrap();
    assert_ne!(listed(&app, "hello")["install"]["bundle"], shown.as_str());

    // a window that shows another version is refused, and nothing is recorded
    let stale = app.reviews().decide(
        &review.id,
        &json!({"ok": true}),
        None,
        Some("0".repeat(64).as_str()),
    );
    assert!(matches!(stale, Err(Error::Conflict(_))), "{stale:?}");
    let decided = app
        .reviews()
        .decide(&review.id, &json!({"ok": true}), None, Some(&shown))
        .unwrap();
    assert_eq!(decided.plugin_bundle.as_deref(), Some(shown.as_str()));
}

/// A new version whose payload schema refuses a pending review's payload
/// leaves the review where it is: it opens with the version it was
/// submitted to, says the installed one does not take it, and can be
/// decided. A version that takes it moves it, with that version's summary.
#[tokio::test]
async fn an_update_moves_the_pending_reviews_it_takes() {
    let dir = tempfile::tempdir().unwrap();
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let sources = dir.path().join("sources");
    copy(&app, &plugin(&sources, "hello", "1.0.0"))
        .await
        .unwrap();
    let review = app
        .reviews()
        .submit(
            &json!({"plugin": "hello", "title": "Keep", "payload": {"items": [1, 2]}}),
            None,
        )
        .unwrap();
    let first = review.plugin_bundle.clone().unwrap();

    // 1.1.0 needs a field the review's payload has not got
    let refusing = plugin(&sources, "hello", "1.1.0");
    std::fs::write(
        refusing.join("schemas/payload.schema.json"),
        json!({"type": "object", "required": ["title"]}).to_string(),
    )
    .unwrap();
    copy(&app, &refusing).await.unwrap();
    let opened = app.reviews().open(&review.id).unwrap();
    assert_eq!(opened.review.plugin_bundle.as_deref(), Some(first.as_str()));
    assert!(
        opened
            .refused
            .as_deref()
            .is_some_and(|why| why.contains("title")),
        "{:?}",
        opened.refused
    );

    // 1.2.0 takes it, and counts its items
    let taking = plugin(&sources, "hello", "1.2.0");
    let manifest = taking.join("manifest.json");
    let mut m: Value = serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    m["summary"] = json!({"request": {"counts": [{"items": "/items", "label": "items"}]}});
    std::fs::write(&manifest, m.to_string()).unwrap();
    copy(&app, &taking).await.unwrap();
    let opened = app.reviews().open(&review.id).unwrap();
    assert!(opened.refused.is_none());
    assert_eq!(opened.review.plugin_version, "1.2.0");
    assert!(
        opened.review.summary.is_some(),
        "{:?}",
        opened.review.summary
    );
    app.reviews()
        .decide(
            &review.id,
            &json!({}),
            None,
            opened.review.plugin_bundle.as_deref(),
        )
        .unwrap();

    // ended, it keeps its version through another update
    let later = plugin(&sources, "hello", "1.3.0");
    copy(&app, &later).await.unwrap();
    let ended = app.reviews().open(&review.id).unwrap();
    assert_eq!(ended.review.plugin_version, "1.2.0");
}

/// The check of linked folders notices a change and says so, storing
/// nothing; the plugin is stored once, when it is next used, however many
/// changes came before.
#[tokio::test]
async fn changes_to_a_linked_folder_are_stored_once_when_it_is_used() {
    let dir = tempfile::tempdir().unwrap();
    let app = Pinrail::open(Config::new(dir.path().join("data"), 0)).unwrap();
    let folder = plugin(&dir.path().join("sources"), "hello", "1.0.0");
    link(&app, &folder).await.unwrap();
    let bundles = stored(&app);
    assert!(
        !app.plugins().reload_if_links_changed().unwrap(),
        "nothing changed"
    );
    assert_eq!(listed(&app, "hello")["install"]["folder_changed"], false);

    let mut notices = app.events().subscribe();
    for i in 0..4 {
        std::fs::write(folder.join("view/index.html"), format!("<html>{i}</html>")).unwrap();
        // a later modification time, as each save gives
        let file = std::fs::File::options()
            .write(true)
            .open(folder.join("view/index.html"))
            .unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(i + 1))
            .unwrap();
        assert!(
            app.plugins().reload_if_links_changed().unwrap(),
            "change {i}"
        );
        assert_eq!(notices.try_recv().unwrap().kind, events::PLUGINS_RELOADED);
        assert!(
            !app.plugins().reload_if_links_changed().unwrap(),
            "said once"
        );
    }
    assert_eq!(stored(&app), bundles, "the check stored a copy");
    assert_eq!(listed(&app, "hello")["install"]["folder_changed"], true);

    app.plugins().describe(Some("hello")).unwrap();
    let after = stored(&app);
    assert_eq!(
        after.len(),
        bundles.len() + 1,
        "one copy for all the changes"
    );
    let row = listed(&app, "hello");
    assert_eq!(row["install"]["folder_changed"], false);
    let view = std::fs::read_to_string(
        app.config()
            .plugin_bundles_dir()
            .join(row["install"]["bundle"].as_str().unwrap())
            .join("view/index.html"),
    )
    .unwrap();
    assert_eq!(view, "<html>3</html>");
}
