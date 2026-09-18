//! Application construction and operations shared by every interface.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::Config;
use crate::db::Db;
use crate::error::Error;
use crate::events::{Bus, Notice};
use crate::plugins::{self as plugin_store, PluginService, Registry};
use crate::reviews::Reviews;
use crate::settings::SettingsService;

/// The running application, shared by the desktop and HTTP interfaces.
/// Opening it initializes local storage and services without starting a server.
#[derive(Debug)]
pub struct Wicket {
    config: Config,
    started_at: DateTime<Utc>,
    events: Bus,
    settings: SettingsService,
    plugins: PluginService,
    db: Arc<Db>,
    reviews: Reviews,
}

impl Wicket {
    /// The configuration this application was opened with.
    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }

    /// The shared channel for review, settings and plugin events.
    pub fn events(&self) -> &Bus {
        &self.events
    }

    /// Recorded events after `after`, in id order, with at most `limit` notices.
    /// Review data reflects its current state and omits the payload. Missing or
    /// unreadable reviews leave the notice's review data empty, as during catch-up.
    pub fn events_after(&self, after: i64, limit: usize) -> Result<Vec<Notice>, Error> {
        Ok(self
            .db
            .events_after(after, limit)?
            .into_iter()
            .map(|event| Notice {
                event_id: event.id,
                kind: event.kind,
                review: event
                    .review_id
                    .as_deref()
                    .and_then(|id| self.db.get_review(id).ok().flatten())
                    .map(|review| review.to_json(false)),
                review_id: event.review_id,
                keys: event
                    .attrs
                    .get("keys")
                    .and_then(Value::as_array)
                    .map(|keys| {
                        keys.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    }),
            })
            .collect())
    }

    /// Review operations share the application's storage, registry and event bus.
    pub fn reviews(&self) -> &Reviews {
        &self.reviews
    }

    /// Settings reads and changes share validation, persistence and notifications.
    pub fn settings(&self) -> &SettingsService {
        &self.settings
    }

    /// Plugin operations share the registry, persistence and change notifications.
    pub fn plugins(&self) -> &PluginService {
        &self.plugins
    }

    /// Opens the database, writes out the built-in plugin, scans the plugin
    /// directories and wires the services together. The caller decides how the
    /// application is held: serving it over HTTP wants an `Arc`, a one-off
    /// operation does not.
    pub fn open(config: Config) -> Result<Self, Error> {
        std::fs::create_dir_all(&config.data_dir)?;
        let db = Arc::new(Db::open(&config.db_path())?);
        let builtin = plugin_store::install_builtin(&config.builtin_plugins_dir())?;
        let user = config.user_plugins_dir();
        let _ = std::fs::create_dir_all(&user);
        let mut defaults = vec![builtin, user];
        defaults.extend(config.plugin_dirs.iter().cloned());
        let records = db.installed_plugins()?;
        if let Some(plugins_dir) = config.plugin_store_dir().parent() {
            plugin_store::tidy(plugins_dir)?;
        }
        let registry = Arc::new(
            Registry::open(defaults, records, config.plugin_store_dir())
                .map_err(Error::Internal)?,
        );
        let events = Bus::new();
        let settings = SettingsService::open(
            &config.data_dir,
            db.clone(),
            registry.clone(),
            events.clone(),
        );
        let plugins = PluginService::new(db.clone(), registry.clone(), events.clone());
        let reviews = Reviews::new(
            db.clone(),
            registry.clone(),
            events.clone(),
            config.user.clone(),
        );
        Ok(Self {
            config,
            started_at: Utc::now(),
            events,
            settings,
            plugins,
            db,
            reviews,
        })
    }
}
