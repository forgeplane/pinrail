//! Application construction and operations shared by every interface.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::Config;
use crate::db::Db;
use crate::error::Error;
use crate::events::Bus;
use crate::plugins::{self as plugin_store, PluginService, Registry};
use crate::reviews::Reviews;
use crate::settings::SettingsService;

/// The running application, shared by the desktop and HTTP interfaces.
/// Opening it initializes local storage and services without starting a server.
#[derive(Debug)]
pub struct Wicket {
    config: Config,
    started_at: DateTime<Utc>,
    settings: SettingsService,
    plugins: PluginService,
    pub(crate) db: Arc<Db>,
    pub(crate) registry: Arc<Registry>,
    reviews: Reviews,
    /// plugin installs under way or done, by job id
    pub(crate) jobs: Arc<crate::install::Jobs>,
}

impl Wicket {
    /// The configuration this application was opened with.
    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
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
    /// directories and wires the service together.
    pub fn open(config: Config) -> Result<Arc<Self>, Error> {
        std::fs::create_dir_all(&config.data_dir)?;
        let db = Arc::new(Db::open(&config.db_path())?);
        let builtin = plugin_store::install_builtin(&config.builtin_plugins_dir())?;
        let user = config.user_plugins_dir();
        let _ = std::fs::create_dir_all(&user);
        let mut defaults = vec![builtin, user];
        defaults.extend(config.plugin_dirs.iter().cloned());
        let records = db.installed_plugins()?;
        if let Some(plugins_dir) = config.plugin_store_dir().parent() {
            crate::install::tidy(plugins_dir)?;
        }
        let registry = Arc::new(
            Registry::open(defaults, records, config.plugin_store_dir())
                .map_err(Error::Internal)?,
        );
        let bus = Bus::new();
        let settings =
            SettingsService::open(&config.data_dir, db.clone(), registry.clone(), bus.clone());
        let plugins = PluginService::new(db.clone(), registry.clone(), bus.clone());
        let reviews = Reviews::new(db.clone(), registry.clone(), bus, config.user.clone());
        Ok(Arc::new(Self {
            config,
            started_at: Utc::now(),
            settings,
            plugins,
            db,
            registry,
            reviews,
            jobs: Arc::new(crate::install::Jobs::default()),
        }))
    }
}
