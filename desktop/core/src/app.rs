//! Application construction and operations shared by every interface.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::Config;
use crate::db::Db;
use crate::error::Error;
use crate::events::{self, Bus};
use crate::plugins::{self as plugin_store, Registry};
use crate::reviews::Reviews;

/// The running application, shared by the desktop and HTTP interfaces.
/// Opening it initializes local storage and services without starting a server.
#[derive(Debug)]
pub struct Wicket {
    pub config: Config,
    pub started_at: DateTime<Utc>,
    pub settings: Arc<crate::settings::Store>,
    pub db: Arc<Db>,
    pub registry: Arc<Registry>,
    pub reviews: Reviews,
    /// plugin installs under way or done, by job id
    pub jobs: Arc<crate::install::Jobs>,
}

impl Wicket {
    /// Applies a partial change to the settings and announces what changed,
    /// for the API and for the app itself (the tray's pause, for one).
    pub fn change_settings(&self, patch: &serde_json::Value) -> Result<serde_json::Value, Error> {
        // a plugin's own settings are the plugin's schema to judge; a name
        // that is not registered now is kept as it is
        let mut violations = Vec::new();
        if let Some(plugins) = patch.get("plugins").and_then(serde_json::Value::as_object) {
            for (name, change) in plugins {
                if let Some(plugin) = self.registry.get(name) {
                    violations.extend(plugin.validate_settings(change));
                }
            }
        }
        if !violations.is_empty() {
            violations.sort_by(|a, b| a.path.cmp(&b.path));
            return Err(Error::Invalid(violations));
        }
        let (after, keys) = self.settings.patch(patch)?;
        if !keys.is_empty() {
            self.announce_settings(&keys)?;
        }
        Ok(after)
    }

    /// Opens the database, writes out the built-in plugin, scans the plugin
    /// directories and wires the service together.
    pub fn open(config: Config) -> Result<Arc<Self>, Error> {
        std::fs::create_dir_all(&config.data_dir)?;
        let settings = Arc::new(crate::settings::Store::open(&config.data_dir));
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
        let reviews = Reviews::new(
            db.clone(),
            registry.clone(),
            Bus::new(),
            config.user.clone(),
        );
        Ok(Arc::new(Self {
            config,
            started_at: Utc::now(),
            settings,
            db,
            registry,
            reviews,
            jobs: Arc::new(crate::install::Jobs::default()),
        }))
    }

    /// Picks up an external settings edit and records and broadcasts its keys.
    /// The host decides when to poll; this operation does not require HTTP.
    pub fn reload_settings(&self) -> Result<Option<Vec<String>>, Error> {
        let keys = self.settings.reload_if_changed();
        if let Some(keys) = &keys {
            self.announce_settings(keys)?;
        }
        Ok(keys)
    }

    /// Records and broadcasts which settings changed, from a patch or from an
    /// edit to the file.
    fn announce_settings(&self, keys: &[String]) -> Result<(), Error> {
        let event_id = self.db.append_event(
            None,
            events::SETTINGS_CHANGED,
            None,
            &serde_json::json!({ "keys": keys }),
        )?;
        self.reviews
            .publish_keys(event_id, events::SETTINGS_CHANGED, keys.to_vec());
        Ok(())
    }
}
