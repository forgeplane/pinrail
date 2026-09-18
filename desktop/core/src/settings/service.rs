//! Settings operations shared by the HTTP API, desktop and file watcher.

use std::sync::Arc;

use serde_json::{Value, json};

use super::Store;
use crate::db::Db;
use crate::error::Error;
use crate::events::{self, Bus, Notice};
use crate::plugins::Registry;

/// Validates and persists settings, then records and broadcasts each change.
#[derive(Debug)]
pub struct SettingsService {
    store: Store,
    db: Arc<Db>,
    registry: Arc<Registry>,
    bus: Bus,
}

impl SettingsService {
    pub(crate) fn new(store: Store, db: Arc<Db>, registry: Arc<Registry>, bus: Bus) -> Self {
        Self {
            store,
            db,
            registry,
            bus,
        }
    }

    /// Every setting: a snapshot of the defaults with the file's values over them.
    pub fn get(&self) -> Value {
        self.store.get()
    }

    /// One setting addressed by a JSON pointer, or null when it is absent.
    pub fn value(&self, pointer: &str) -> Value {
        self.store.value(pointer)
    }

    /// Applies a partial change to the settings and announces what changed,
    /// for the API and for the app itself (the tray's pause, for one).
    pub fn change(&self, patch: &Value) -> Result<Value, Error> {
        // a plugin's own settings are the plugin's schema to judge; a name
        // that is not registered now is kept as it is
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
        let event_id = self.db.append_event(
            None,
            events::SETTINGS_CHANGED,
            None,
            &json!({ "keys": keys }),
        )?;
        self.bus.publish(Notice {
            event_id,
            kind: events::SETTINGS_CHANGED.to_string(),
            review_id: None,
            review: None,
            keys: Some(keys.to_vec()),
        });
        Ok(())
    }
}
