//! Plugin operations shared by the HTTP API and other application interfaces.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};

use super::{Plugin, Registry};
use crate::db::Db;
use crate::error::Error;
use crate::events::{self, Bus, Notice};

#[derive(Debug, Clone)]
pub struct PluginService {
    db: Arc<Db>,
    registry: Arc<Registry>,
    bus: Bus,
}

impl PluginService {
    pub(crate) fn new(db: Arc<Db>, registry: Arc<Registry>, bus: Bus) -> Self {
        Self { db, registry, bus }
    }

    pub fn dirs(&self) -> Vec<PathBuf> {
        self.registry.dirs()
    }

    /// Registered plugins with their effective settings over the supplied stored values.
    pub fn listing(&self, stored: &Value) -> Value {
        let plugins = self
            .registry
            .all()
            .iter()
            .map(|p| {
                let mut row = p.to_json();
                // the plugin's settings as they stand: defaults under the stored values
                row["settings"] = p
                    .has_settings()
                    .then(|| p.effective_settings(&stored[&p.name]))
                    .into();
                row
            })
            .collect::<Vec<_>>();
        json!({
            "dirs": self.registry.dirs().iter().map(|d| d.display().to_string()).collect::<Vec<_>>(),
            "plugins": plugins,
        })
    }

    /// Available major versions, including entries retained for existing reviews.
    pub fn versions(&self, name: &str) -> Result<Value, Error> {
        // the majors that render: the current plugin, and the store entries
        // kept for reviews that still point at them
        let current = self.registry.get(name);
        let mut versions: Vec<u32> = Vec::new();
        let dir = self.registry.store_dir().join(name);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Ok(v) = entry.file_name().to_string_lossy().parse::<u32>()
                    && entry.path().join("manifest.json").is_file()
                {
                    versions.push(v);
                }
            }
        }
        if let Some(p) = &current
            && p.usable()
            && !versions.contains(&p.version)
        {
            versions.push(p.version);
        }
        if current.is_none() && versions.is_empty() {
            return Err(Error::NotFound(name.to_string()));
        }
        versions.sort_unstable();
        Ok(json!({
            "name": name,
            "current": current.as_ref().filter(|p| p.usable()).map(|p| p.version),
            "versions": versions,
        }))
    }

    /// The version a review renders with, even after its plugin was removed.
    pub fn fetch_version(&self, name: &str, version: u32) -> Result<Arc<Plugin>, Error> {
        self.registry.fetch_version(name, version)
    }

    /// Removes the installation while retaining versions still used by reviews.
    pub fn remove(&self, name: &str) -> Result<Value, Error> {
        let answer = crate::install::remove(&self.db, &self.registry, name)?;
        self.announce()?;
        Ok(answer)
    }

    pub fn reload(&self) -> Result<usize, Error> {
        let records = self.db.installed_plugins()?;
        let count = self
            .registry
            .reload_with(records)
            .map_err(Error::Internal)?;
        self.announce()?;
        Ok(count)
    }

    /// Links plugins not yet installed. Conflicting definitions refuse the directory.
    pub fn add_dir(&self, dir: &Path) -> Result<usize, Error> {
        // New plugins become links served live from their source directories.
        let links = self
            .registry
            .link_all(dir)
            .map_err(|message| Error::invalid("/dir", message))?;
        let mut records = self.db.installed_plugins()?;
        records.extend(links.iter().cloned());
        let count = match self.registry.reload_with(records) {
            Ok(count) => count,
            Err(message) => {
                let _ = self.registry.reload_with(self.db.installed_plugins()?);
                return Err(Error::invalid("/dir", message));
            }
        };
        for record in &links {
            self.db.upsert_installed(record)?;
        }
        self.announce()?;
        Ok(count)
    }

    pub(crate) fn announce(&self) -> Result<(), Error> {
        let event_id = self
            .db
            .append_event(None, events::PLUGINS_RELOADED, None, &Value::Null)?;
        self.bus.publish(Notice {
            event_id,
            kind: events::PLUGINS_RELOADED.to_string(),
            review_id: None,
            review: None,
            keys: None,
        });
        Ok(())
    }
}
