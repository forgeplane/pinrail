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
use crate::plugins::{self as plugin_store, Bundles, PluginService, Registry};
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
        for key in ["title", "requested_by", "origin"] {
            if let Some(value) = overrides.get(key).filter(|v| !v.is_null()) {
                body[key] = value.clone();
            }
        }
        self.reviews.submit(&body, None)
    }

    /// Locks the data directory, opens the database, stores the plugins the
    /// app carries, loads the installed ones and wires the services
    /// together. The caller decides how the application is held: serving it
    /// over HTTP wants an `Arc`, a one-off operation does not.
    ///
    /// A directory another Pinrail has open is refused with
    /// [`Error::InUse`] before anything in it is touched.
    pub fn open(config: Config) -> Result<Self, Error> {
        private_dir(&config.data_dir)?;
        let lock = lock_data_dir(&config.data_dir)?;
        let db = Arc::new(Db::open(&config.db_path())?);
        plugin_store::tidy(&config.plugins_dir())?;
        let bundles = Bundles::open(&config.plugin_bundles_dir(), db.clone())?;
        plugin_store::store_bundled(&db, &bundles)?;
        let registry = Arc::new(
            Registry::open(db.clone(), bundles.clone(), config.plugins_dir())?
                .with_build_timeout(config.build_timeout),
        );
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
    use crate::plugins::{bundled, store_releases};
    use serde_json::json;

    /// The list plugin as a later app would ship it: 2.0.0, a new line,
    /// whose decisions have another shape.
    fn list_2() -> Vec<(String, crate::plugins::bundles::Files)> {
        bundled()
            .into_iter()
            .filter(|(folder, _)| folder == "list")
            .map(|(folder, files)| {
                let files = files
                    .into_iter()
                    .map(|(path, bytes)| match path.as_str() {
                        "manifest.json" => {
                            let text = String::from_utf8(bytes).unwrap();
                            let bytes = text.replace("\"1.0.0\"", "\"2.0.0\"").into_bytes();
                            (path, bytes)
                        }
                        "schemas/decision.schema.json" => (
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
            .collect()
    }

    /// The next start of the app, with `plugins` among the ones it ships.
    fn start_with(
        config: &Config,
        plugins: Vec<(String, crate::plugins::bundles::Files)>,
    ) -> Pinrail {
        let db = Db::open(&config.db_path()).unwrap();
        let bundles = Bundles::open(
            &config.plugin_bundles_dir(),
            Arc::new(Db::open(&config.db_path()).unwrap()),
        )
        .unwrap();
        store_releases(&db, &bundles, plugins).unwrap();
        drop((db, bundles));
        Pinrail::open(config.clone()).unwrap()
    }

    /// A review renders and decides with the release it was submitted to
    /// when the app ships the next, whatever that changes, and new reviews
    /// use the new release, also after an older app opened the data.
    #[test]
    fn a_review_keeps_its_release_when_the_app_ships_the_next() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::new(dir.path(), 0);
        let app = Pinrail::open(config.clone()).unwrap();
        let payload = json!({"groups": [{"title": "g", "items": [{"id": 1, "title": "one"}]}]});
        let old = app
            .reviews()
            .submit(
                &json!({"plugin": "list", "title": "With 1.0.0", "payload": payload}),
                None,
            )
            .unwrap();
        assert_eq!(old.plugin_version, "1.0.0");
        drop(app);

        // the next app ships list 2.0.0; then an older app opens the data
        drop(start_with(&config, list_2()));
        let app = Pinrail::open(config).unwrap();
        let new = app
            .reviews()
            .submit(
                &json!({"plugin": "list", "title": "With 2.0.0", "payload": payload}),
                None,
            )
            .unwrap();
        assert_eq!(new.plugin_version, "2.0.0");
        assert_ne!(new.plugin_bundle, old.plugin_bundle);

        // each decided with its own release's schema
        let one = json!({"decisions": [{"id": 1, "action": "accept"}], "undecided": []});
        assert!(app.reviews().decide(&new.id, &one, None).is_err());
        app.reviews().decide(&old.id, &one, None).unwrap();
        app.reviews()
            .decide(&new.id, &json!({"verdict": "ok"}), None)
            .unwrap();
        let rendered = app
            .plugins()
            .fetch_review(&old.plugin, old.plugin_bundle.as_deref())
            .unwrap();
        assert_eq!(rendered.version, "1.0.0");
    }

    /// A patch the app ships is what new reviews use; the reviews made
    /// before keep the release they were made with.
    #[test]
    fn a_newer_bundled_release_is_for_new_reviews() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::new(dir.path(), 0);
        let app = Pinrail::open(config.clone()).unwrap();
        let review = app
            .reviews()
            .submit(
                &json!({"plugin": "list", "title": "Before", "payload": {"groups": []}}),
                None,
            )
            .unwrap();
        drop(app);

        let patched: Vec<_> = bundled()
            .into_iter()
            .filter(|(folder, _)| folder == "list")
            .map(|(folder, files)| {
                let files = files
                    .into_iter()
                    .map(|(path, bytes)| {
                        if path == "manifest.json" {
                            let text = String::from_utf8(bytes).unwrap();
                            (path, text.replace("\"1.0.0\"", "\"1.0.1\"").into_bytes())
                        } else {
                            (path, bytes)
                        }
                    })
                    .collect();
                (folder, files)
            })
            .collect();
        let app = start_with(&config, patched);
        let new = app
            .reviews()
            .submit(
                &json!({"plugin": "list", "title": "After", "payload": {"groups": []}}),
                None,
            )
            .unwrap();
        assert_eq!(new.plugin_version, "1.0.1");
        let kept = app
            .plugins()
            .fetch_review(&review.plugin, review.plugin_bundle.as_deref())
            .unwrap();
        assert_eq!(kept.version, "1.0.0");
    }
}
