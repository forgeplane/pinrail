//! Plugin operations shared by the HTTP API and other application interfaces.

use std::sync::Arc;

use serde_json::{Value, json};

use super::{InstallOptions, Plugin, Registry, install};
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

    /// Each usable plugin described for an agent, or the one named.
    pub fn describe(&self, name: Option<&str>) -> Result<Value, Error> {
        let wanted = name
            .map(|n| self.registry.resolve(n))
            .transpose()
            .ok()
            .flatten();
        let plugins: Vec<Value> = self
            .registry
            .all()
            .iter()
            .filter(|p| p.usable() && name.is_none_or(|_| wanted.as_deref() == Some(p.full_name())))
            .map(|p| p.describe())
            .collect();
        match name {
            Some(n) if plugins.is_empty() => Err(Error::NotFound(format!("plugin {n}"))),
            _ => Ok(json!({ "plugins": plugins })),
        }
    }

    /// The sample of a usable plugin, the one named or else its first, or
    /// why there is none to send.
    pub fn sample(&self, name: &str, which: Option<&str>) -> Result<super::Sample, Error> {
        let plugin = self
            .registry
            .get(name)
            .filter(|p| p.usable())
            .ok_or_else(|| {
                Error::invalid("/plugin", format!("no usable plugin is named {name}"))
            })?;
        if let Some(sample) = plugin.sample(which) {
            return Ok(sample.clone());
        }
        let names = plugin.sample_names();
        Err(Error::invalid(
            "/sample",
            match (which, names.is_empty(), plugin.sample_errors.first()) {
                (Some(which), false, _) => format!(
                    "{name} has no sample named {which}; its samples are {}",
                    names.join(", ")
                ),
                (_, true, Some(why)) => format!("{name}'s sample does not load: {why}"),
                _ => format!("{name} has no sample"),
            },
        ))
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
                    .then(|| p.effective_settings(&stored[p.full_name()]))
                    .into();
                row
            })
            .collect::<Vec<_>>();
        json!({ "plugins": plugins })
    }

    /// The folder of a linked plugin, by full name; none for any other.
    pub fn installed_link(&self, plugin: &str) -> Option<std::path::PathBuf> {
        self.registry
            .installs()
            .into_iter()
            .find(|i| i.plugin == plugin && i.linked())
            .map(|i| std::path::PathBuf::from(i.resolved))
    }

    /// The full name a plugin's name stands for; see [`Registry::resolve`].
    pub fn resolve(&self, name: &str) -> Result<String, Error> {
        self.registry.resolve(name)
    }

    /// The plugin a review renders with; see [`Registry::fetch_review`].
    pub fn fetch_review(&self, plugin: &str, bundle: Option<&str>) -> Result<Arc<Plugin>, Error> {
        self.registry.fetch_review(plugin, bundle)
    }

    /// Removes the installation; its reviews keep the bundles they were
    /// submitted to.
    pub fn remove(&self, name: &str) -> Result<Value, Error> {
        let answer = install::remove(&self.db, &self.registry, name)?;
        self.announce()?;
        Ok(answer)
    }

    /// Reads the plugins again when a linked folder changed, and announces
    /// it; whether it did. The server asks every second, so a change to a
    /// plugin being developed applies without a reload.
    pub fn reload_if_links_changed(&self) -> Result<bool, Error> {
        if !self.registry.links_changed() {
            return Ok(false);
        }
        self.registry.reload()?;
        self.announce()?;
        Ok(true)
    }

    pub fn reload(&self) -> Result<usize, Error> {
        let count = self.registry.reload()?;
        self.announce()?;
        Ok(count)
    }

    /// Inspects a source without installing it or announcing a change.
    pub async fn inspect(&self, source: &str, options: InstallOptions) -> Result<Value, Error> {
        let worker = self.clone();
        let source = source.to_string();
        tokio::task::spawn_blocking(move || {
            install::inspect(&worker.db, &worker.registry, &source, options)
        })
        .await
        .map_err(|error| Error::Internal(error.to_string()))?
    }

    /// Installs the plugin a source holds and announces it; the plugin's
    /// row, as the listing shows it.
    pub async fn install(&self, source: &str, options: InstallOptions) -> Result<Value, Error> {
        let worker = self.clone();
        let source = source.to_string();
        let record = tokio::task::spawn_blocking(move || {
            install::install(&worker.db, &worker.registry, &source, options)
        })
        .await
        .map_err(|error| Error::Internal(error.to_string()))??;
        self.announce()?;
        self.registry
            .get(&record.plugin)
            .map(|p| p.to_json())
            .ok_or_else(|| {
                Error::Internal(format!(
                    "{} was installed and is not registered",
                    record.plugin
                ))
            })
    }

    fn announce(&self) -> Result<(), Error> {
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
