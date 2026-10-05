//! Application construction and operations shared by every interface.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use crate::Config;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::attachments::{Attachments, UploadError};
use crate::db::Db;
use crate::error::Error;
use crate::events::Events;
use crate::plugins::{self as plugin_store, Bundles, Catalog, PluginService, Registry};
use crate::reviews::Reviews;
use crate::settings::SettingsService;

/// The running application, shared by the desktop and HTTP interfaces.
/// Opening it initializes local storage and services without starting a
/// server. It holds the configuration, the lock on the data directory and
/// the services, and the services own the storage.
#[derive(Debug)]
pub struct Pinrail {
    config: Config,
    events: Events,
    settings: SettingsService,
    plugins: PluginService,
    reviews: Reviews,
    attachments: Attachments,
    bundles: Bundles,
    /// The data directory's lock, held until the application is dropped
    _lock: File,
}

impl Pinrail {
    /// The configuration this application was opened with.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The shared channel for review, settings and plugin events.
    /// Who hears what happened, and what a client that was away missed.
    pub fn events(&self) -> &Events {
        &self.events
    }

    /// Review operations share the application's storage, registry and event bus.
    pub fn reviews(&self) -> &Reviews {
        &self.reviews
    }

    /// The plugin bundles the app holds, by hash.
    pub fn bundles(&self) -> &Bundles {
        &self.bundles
    }

    /// The files sent beside reviews, stored by their hash.
    pub fn attachments(&self) -> &Attachments {
        &self.attachments
    }

    /// Settings reads and changes share validation, persistence and notifications.
    pub fn settings(&self) -> &SettingsService {
        &self.settings
    }

    /// Plugin operations share the registry, persistence and change notifications.
    pub fn plugins(&self) -> &PluginService {
        &self.plugins
    }

    /// Sends a plugin's sample as a new review: its files stored, its
    /// request submitted like any other. `overrides` may name the `sample`
    /// (the first otherwise) and give a `title`, `requested_by` or
    /// `origin`; the sample's are used otherwise.
    pub fn send_sample(
        &self,
        name: &str,
        overrides: &Value,
    ) -> Result<crate::reviews::Review, Error> {
        let which = overrides.get("sample").and_then(Value::as_str);
        let sample = self.plugins.sample(name, which)?;
        let mut stored = Vec::new();
        for file in &sample.files {
            let bytes = std::fs::read(&file.path)?;
            let sha256 = format!("{:x}", Sha256::digest(&bytes));
            if self.attachments.stored(&sha256)?.is_none() {
                let mut upload = self.attachments.begin(&sha256)?;
                let too_large = |_| {
                    Error::invalid(
                        format!("/attachments/{}", file.name),
                        "is larger than the app takes",
                    )
                };
                upload.write(&bytes).map_err(|e| match e {
                    UploadError::Failed(error) => error,
                    other => too_large(other),
                })?;
                upload.finish().map_err(|e| match e {
                    UploadError::Failed(error) => error,
                    other => too_large(other),
                })?;
            }
            stored.push((sha256, bytes.len() as u64));
        }
        let mut body = sample.request(name, &stored);
        for key in ["title", "requested_by", "origin", "session"] {
            if let Some(value) = overrides.get(key).filter(|v| !v.is_null()) {
                body[key] = value.clone();
            }
        }
        self.reviews.submit(&body, None)
    }

    /// Locks the data directory, opens the database, loads the installed
    /// plugins with the catalog of those the app carries, and wires the
    /// services together. The caller decides how the application is held:
    /// serving it over HTTP wants an `Arc`, a one-off operation does not.
    ///
    /// A directory another Pinrail has open is refused with
    /// [`Error::InUse`] before anything in it is touched.
    pub fn open(config: Config) -> Result<Self, Error> {
        let mut catalogs = vec![Catalog::builtin()];
        if let Some(dir) = &config.catalog_dir {
            catalogs.push(Catalog::of_dir(dir)?);
        }
        Self::open_with(config, catalogs)
    }

    /// [`Pinrail::open`] with these catalogs to install and update from.
    pub(crate) fn open_with(config: Config, catalogs: Vec<Catalog>) -> Result<Self, Error> {
        private_dir(&config.data_dir)?;
        let lock = lock_data_dir(&config.data_dir)?;
        let db = Arc::new(Db::open(&config.db_path())?);
        plugin_store::tidy(&config.plugins_dir())?;
        let bundles = Bundles::open(&config.plugin_bundles_dir(), db.clone())?;
        let registry = Arc::new(Registry::open(
            db.clone(),
            bundles.clone(),
            config.plugins_dir(),
            catalogs,
        )?);
        let events = Events::new(db.clone());
        let settings =
            SettingsService::open(&config.data_dir, db.clone(), registry.clone(), events.bus());
        let plugins = PluginService::new(db.clone(), registry.clone(), events.bus());
        let attachments = Attachments::open(
            &config.attachments_dir(),
            db.clone(),
            config.max_attachment_bytes,
        )?;
        let reviews = Reviews::new(
            db.clone(),
            registry.clone(),
            events.bus(),
            config.user.clone(),
            attachments.clone(),
        );
        Ok(Self {
            config,
            events,
            settings,
            plugins,
            reviews,
            attachments,
            bundles,
            _lock: lock,
        })
    }
}

/// Takes `pinrail.lock` in the data directory, and writes this process's id
/// into it for the message another open shows. Opening cleans up what a
/// stopped server left behind, which for a running one is its uploads and
/// installs in progress, so only one application may have the directory
/// open. The operating system releases the lock when the process ends,
/// however it ends.
fn lock_data_dir(dir: &Path) -> Result<File, Error> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("pinrail.lock"))?;
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            let who = match locked_by(dir) {
                Some(pid) => format!("Another Pinrail (process {pid})"),
                None => "Another Pinrail".to_string(),
            };
            return Err(Error::InUse(format!("{who} is using {}", dir.display())));
        }
        Err(TryLockError::Error(error)) => return Err(error.into()),
    }
    file.set_len(0)?;
    file.write_all(std::process::id().to_string().as_bytes())?;
    Ok(file)
}

/// The process that holds the data directory's lock, as it wrote itself
/// into the lock file: the one to stop when [`Pinrail::open`] answers
/// [`Error::InUse`]. `None` when the file is missing or names no process.
pub fn locked_by(dir: &Path) -> Option<u32> {
    let mut holder = String::new();
    File::open(dir.join("pinrail.lock"))
        .ok()?
        .read_to_string(&mut holder)
        .ok()?;
    holder.trim().parse().ok()
}

/// Creates the data directory readable by its owner only, and closes an
/// existing one that others can read: it holds every review, decision and
/// file, and everything in it is created beneath it.
fn private_dir(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(dir)?.permissions().mode() & 0o077 != 0 {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::carried;
    use serde_json::json;

    /// The app's catalog with `list` at another version; `new_line` also
    /// gives its decisions another shape, as a new major version may.
    fn with_list(version: &str, new_line: bool) -> Catalog {
        let plugins = carried()
            .into_iter()
            .map(|(folder, files)| {
                if folder != "list" {
                    return (folder, files);
                }
                let files = files
                    .into_iter()
                    .map(|(path, bytes)| match path.as_str() {
                        "manifest.json" => {
                            let text = String::from_utf8(bytes).unwrap();
                            let bumped = text.replace("\"1.0.0\"", &format!("\"{version}\""));
                            (path, bumped.into_bytes())
                        }
                        "schemas/decision.schema.json" if new_line => (
                            path,
                            json!({"type": "object", "required": ["verdict"]})
                                .to_string()
                                .into_bytes(),
                        ),
                        _ => (path, bytes),
                    })
                    .collect();
                (folder, files)
            })
            .collect();
        Catalog::of(plugins)
    }

    fn install(app: &Pinrail, id: &str) -> Value {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(app.plugins().install_offered(id))
            .unwrap()
    }

    fn row(app: &Pinrail, name: &str) -> Value {
        app.plugins().listing(&json!({}))["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .cloned()
            .unwrap_or(Value::Null)
    }

    /// The app carries its official plugins and installs none of them:
    /// the person chooses.
    #[test]
    fn nothing_is_installed_until_the_person_chooses_it() {
        let dir = tempfile::tempdir().unwrap();
        let app = Pinrail::open(Config::new(dir.path(), 0)).unwrap();
        assert_eq!(app.plugins().listing(&json!({}))["plugins"], json!([]));
        let catalog = app.plugins().catalog();
        assert_eq!(catalog["format"], 1);
        assert_eq!(catalog["plugins"][0]["needs"], Value::Null);
        let offered: Vec<(&str, &Value)> = catalog["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["id"].as_str().unwrap(), &p["installed"]))
            .collect();
        assert_eq!(
            offered,
            [
                ("forgeplane/feedback", &Value::Null),
                ("forgeplane/list", &Value::Null)
            ]
        );

        let installed = install(&app, "forgeplane/list");
        assert_eq!(installed["install"]["source_kind"], "index");
        assert_eq!(installed["install"]["source"], "forgeplane/list");
        assert_eq!(app.plugins().catalog()["plugins"][1]["installed"], "1.0.0");
        // an agent asking with it is told how to install it
        for error in [
            app.reviews()
                .submit(
                    &json!({"plugin": "feedback", "title": "Not installed", "payload": {}}),
                    None,
                )
                .unwrap_err(),
            app.send_sample("feedback", &json!({})).unwrap_err(),
        ] {
            assert!(
                error
                    .to_string()
                    .contains("pinrail plugins install feedback"),
                "{error}"
            );
        }
    }

    /// The next app carries list 2.0.0: it is offered as an update, not
    /// applied. Once the person updates, new reviews use it, and each review
    /// keeps the release it was submitted to. An older app opening the data
    /// again offers nothing and changes nothing.
    #[test]
    fn a_newer_release_in_the_next_app_is_offered_as_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::new(dir.path(), 0);
        let app = Pinrail::open(config.clone()).unwrap();
        install(&app, "list");
        let payload = json!({"groups": [{"title": "g", "items": [{"id": 1, "title": "one"}]}]});
        let submit = |app: &Pinrail, title: &str| {
            app.reviews()
                .submit(
                    &json!({"plugin": "list", "title": title, "payload": payload}),
                    None,
                )
                .unwrap()
        };
        let old = submit(&app, "With 1.0.0");
        assert_eq!(old.plugin_version, "1.0.0");
        assert_eq!(row(&app, "list")["update"], Value::Null);
        drop(app);

        let app = Pinrail::open_with(config.clone(), vec![with_list("2.0.0", true)]).unwrap();
        assert_eq!(row(&app, "list")["update"], "2.0.0");
        assert_eq!(submit(&app, "Still 1.0.0").plugin_version, "1.0.0");

        let updated = install(&app, "list");
        assert_eq!(updated["version"], "2.0.0");
        assert_eq!(updated["replaced_version"], "1.0.0");
        assert_eq!(row(&app, "list")["update"], Value::Null);
        let new = submit(&app, "With 2.0.0");
        assert_eq!(new.plugin_version, "2.0.0");
        assert_ne!(new.plugin_bundle, old.plugin_bundle);

        // each decided with its own release's schema
        let one = json!({"decisions": [{"id": 1, "action": "accept"}], "undecided": []});
        assert!(app.reviews().decide(&new.id, &one, None, None).is_err());
        app.reviews().decide(&old.id, &one, None, None).unwrap();
        app.reviews()
            .decide(&new.id, &json!({"verdict": "ok"}), None, None)
            .unwrap();
        let rendered = app
            .plugins()
            .fetch_review(&old.plugin, old.plugin_bundle.as_deref())
            .unwrap();
        assert_eq!(rendered.version, "1.0.0");
        drop(app);

        let app = Pinrail::open(config).unwrap();
        assert_eq!(row(&app, "list")["version"], "2.0.0");
        assert_eq!(row(&app, "list")["update"], Value::Null);
    }

    /// The update is the highest version any catalog offers, as it will be
    /// with the registry's index beside the app's catalog; a plugin a
    /// catalog drops stays installed.
    #[test]
    fn the_highest_version_of_any_catalog_is_the_update() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::new(dir.path(), 0);
        let app = Pinrail::open(config.clone()).unwrap();
        let from_one = install(&app, "list")["install"].clone();
        drop(app);

        // the same release from another catalog makes the same record
        let app = Pinrail::open_with(config.clone(), vec![with_list("1.0.0", false)]).unwrap();
        let from_other = install(&app, "forgeplane/list")["install"].clone();
        for key in ["source_kind", "source", "bundle", "link"] {
            assert_eq!(from_one[key], from_other[key], "{key}");
        }
        drop(app);

        let catalogs = vec![
            Catalog::builtin(),
            with_list("1.4.0", false),
            with_list("1.2.0", false),
        ];
        let app = Pinrail::open_with(config.clone(), catalogs).unwrap();
        assert_eq!(row(&app, "list")["update"], "1.4.0");
        drop(app);

        let app = Pinrail::open_with(config, vec![]).unwrap();
        assert_eq!(row(&app, "list")["version"], "1.0.0");
        assert_eq!(row(&app, "list")["update"], Value::Null);
    }

    /// A version a catalog offers can be asked for by its number, an older
    /// one too; one no catalog offers is refused.
    #[test]
    fn an_offered_version_is_installed_when_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let catalogs = vec![Catalog::builtin(), with_list("1.4.0", false)];
        let app = Pinrail::open_with(Config::new(dir.path(), 0), catalogs).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let at = |version: &str| {
            runtime.block_on(app.plugins().install_offered_at("list", Some(version)))
        };
        assert_eq!(at("1.0.0").unwrap()["version"], "1.0.0");
        assert_eq!(row(&app, "list")["update"], "1.4.0");
        assert!(at("1.2.0").is_err());
        assert_eq!(at("1.4.0").unwrap()["version"], "1.4.0");
    }
}
