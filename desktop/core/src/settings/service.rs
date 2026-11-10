//! Settings operations shared by the HTTP API, desktop and file watcher.

use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::store::Store;
use crate::db::Db;
use crate::error::Error;
use crate::events::{self, Bus, Notice};
use crate::plugins::Registry;

/// What runs after the settings changed, with the pointers that did.
type Follower = Box<dyn Fn(&[String]) + Send + Sync>;

/// Validates and persists settings, then records and broadcasts each change.
pub struct SettingsService {
    store: Arc<Store>,
    db: Arc<Db>,
    registry: Arc<Registry>,
    bus: Bus,
    followers: Mutex<Vec<Follower>>,
}

impl std::fmt::Debug for SettingsService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsService")
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

impl SettingsService {
    /// Opens the settings store and connects validation and change notifications.
    pub(crate) fn open(data_dir: &Path, db: Arc<Db>, registry: Arc<Registry>, bus: Bus) -> Self {
        Self {
            store: Arc::new(Store::open(data_dir)),
            db,
            registry,
            bus,
            followers: Mutex::new(Vec::new()),
        }
    }

    /// Runs `follower` after every change of the settings, in the app or
    /// in the file, at once rather than through the bus, for a service
    /// that must act before anything else reads the settings.
    pub(crate) fn follow(&self, follower: impl Fn(&[String]) + Send + Sync + 'static) {
        self.lock_followers().push(Box::new(follower));
    }

    fn lock_followers(&self) -> std::sync::MutexGuard<'_, Vec<Follower>> {
        self.followers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Every setting: a snapshot of the defaults with the file's values over them.
    pub fn get(&self) -> Value {
        self.store.get()
    }

    /// One setting addressed by a JSON pointer, or null when it is absent.
    pub fn value(&self, pointer: &str) -> Value {
        self.store.value(pointer)
    }

    /// A handle that reads the settings, for a service that follows one.
    pub(crate) fn reader(&self) -> SettingsReader {
        SettingsReader(self.store.clone())
    }

    /// Applies a partial change to the settings and announces what changed,
    /// for the API and for the app itself (the tray's pause, for one).
    pub fn change(&self, patch: &Value) -> Result<Value, Error> {
        // a plugin's own settings are the plugin's schema to judge; a name
        // that is not installed now is kept as it is
        let mut violations = Vec::new();
        if let Some(plugins) = patch.get("plugins").and_then(Value::as_object) {
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
        let (after, keys) = self.store.patch(patch)?;
        if !keys.is_empty() {
            self.announce(&keys)?;
        }
        Ok(after)
    }

    /// Picks up an external settings edit and records and broadcasts its keys.
    /// The host decides when to poll; this operation does not require HTTP.
    pub fn reload(&self) -> Result<Option<Vec<String>>, Error> {
        let keys = self.store.reload_if_changed();
        if let Some(keys) = &keys {
            self.announce(keys)?;
        }
        Ok(keys)
    }

    fn announce(&self, keys: &[String]) -> Result<(), Error> {
        for follower in self.lock_followers().iter() {
            follower(keys);
        }
        let attrs = json!({ "keys": keys });
        let event_id = self
            .db
            .append_event(None, events::SETTINGS_CHANGED, None, &attrs)?;
        self.bus
            .publish(Notice::new(event_id, events::SETTINGS_CHANGED, &attrs));
        Ok(())
    }
}

/// Reads the settings, as they are now, for a service that follows one.
#[derive(Debug, Clone)]
pub(crate) struct SettingsReader(Arc<Store>);

impl SettingsReader {
    /// One setting addressed by a JSON pointer, or null when it is absent.
    pub fn value(&self, pointer: &str) -> Value {
        self.0.value(pointer)
    }
}
