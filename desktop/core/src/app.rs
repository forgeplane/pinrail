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

    /// Locks the data directory, opens the database, writes out the
    /// built-in plugins, loads the installed ones and wires the services
    /// together. The caller decides how the application is held: serving it
    /// over HTTP wants an `Arc`, a one-off operation does not.
    ///
    /// A directory another Pinrail has open is refused with
    /// [`Error::InUse`] before anything in it is touched.
    pub fn open(config: Config) -> Result<Self, Error> {
        private_dir(&config.data_dir)?;
        let lock = lock_data_dir(&config.data_dir)?;
        let db = Arc::new(Db::open(&config.db_path())?);
        let builtin = plugin_store::install_builtin(&config.builtin_plugins_dir())?;
        let records = db.installed_plugins()?;
        if let Some(plugins_dir) = config.plugin_store_dir().parent() {
            plugin_store::tidy(plugins_dir)?;
        }
        let registry = Arc::new(
            Registry::open(builtin, records, config.plugin_store_dir())
                .map_err(Error::Internal)?
                .with_github_api(&config.github_api)
                .with_build_timeout(config.build_timeout)
                .with_fetch_timeout(config.fetch_timeout),
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
        let bundles = Bundles::open(&config.plugin_bundles_dir(), db.clone())?;
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
