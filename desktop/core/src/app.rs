//! Application construction and operations shared by every interface.

use std::sync::Arc;

use crate::Config;
use crate::db::Db;
use crate::error::Error;
use crate::events::Events;
use crate::plugins::{self as plugin_store, PluginService, Registry};
use crate::reviews::Reviews;
use crate::settings::SettingsService;

/// The running application, shared by the desktop and HTTP interfaces.
/// Opening it initializes local storage and services without starting a
/// server. It holds the services and nothing else: storage is theirs.
#[derive(Debug)]
pub struct Pinrail {
    config: Config,
    events: Events,
    settings: SettingsService,
    plugins: PluginService,
    reviews: Reviews,
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
        let records = db.installed_plugins()?;
        if let Some(plugins_dir) = config.plugin_store_dir().parent() {
            plugin_store::tidy(plugins_dir)?;
        }
        let registry = Arc::new(
            Registry::open(builtin, records, config.plugin_store_dir()).map_err(Error::Internal)?,
        );
        let events = Events::new(db.clone());
        let settings =
            SettingsService::open(&config.data_dir, db.clone(), registry.clone(), events.bus());
        let plugins = PluginService::new(db.clone(), registry.clone(), events.bus());
        let reviews = Reviews::new(
            db.clone(),
            registry.clone(),
            events.bus(),
            config.user.clone(),
        );
        Ok(Self {
            config,
            events,
            settings,
            plugins,
            reviews,
        })
    }
}
