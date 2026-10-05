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
        // a linked plugin is described as its folder is now
        match name {
            Some(name) => {
                if self.registry.capture(name)? {
                    self.announce()?;
                }
                if let Some(broken) = self.registry.get(name).filter(|p| !p.usable()) {
                    return Err(Error::invalid(
                        "/plugin",
                        format!(
                            "plugin {name} is not usable: {}",
                            broken.error.clone().unwrap_or_default()
                        ),
                    ));
                }
            }
            None => {
                let mut changed = false;
                for linked in self
                    .registry
                    .all()
                    .iter()
                    .filter(|p| p.install.as_ref().is_some_and(|i| i.link))
                {
                    changed |= self.registry.capture(&linked.name)?;
                }
                if changed {
                    self.announce()?;
                }
            }
        }
        let plugins: Vec<Value> = self
            .registry
            .all()
            .iter()
            .filter(|p| p.usable() && name.is_none_or(|n| p.name == n))
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
        let plugin = match self.registry.get(name) {
            Some(plugin) if plugin.usable() => plugin,
            None if self.registry.offered(name).is_some() => {
                return Err(self.registry.missing(name));
            }
            _ => {
                return Err(Error::invalid(
                    "/plugin",
                    format!("no usable plugin is named {name}"),
                ));
            }
        };
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
                // the version a catalog offers it at, when that is newer
                row["update"] = self
                    .registry
                    .update_for(&p.name)
                    .map(|entry| entry.version.clone())
                    .into();
                // the plugin's settings as they stand: defaults under the stored values
                row["settings"] = p
                    .has_settings()
                    .then(|| p.effective_settings(&stored[&p.name]))
                    .into();
                row
            })
            .collect::<Vec<_>>();
        json!({ "plugins": plugins })
    }

    /// The official plugins the app can install, as the registry's
    /// compiled index lists them, each with the version installed, if any.
    pub fn catalog(&self) -> Value {
        let plugins: Vec<Value> = self
            .registry
            .catalog()
            .into_iter()
            .map(|entry| {
                let mut row = entry.to_json();
                row["installed"] = self
                    .registry
                    .get(&entry.name)
                    .filter(|p| {
                        p.install
                            .as_ref()
                            .is_some_and(|i| i.source_kind == "index" && i.source == entry.id())
                    })
                    .map(|p| p.version.clone())
                    .into();
                row
            })
            .collect();
        json!({ "format": 1, "plugins": plugins })
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

    /// Notices each linked folder that changed since it was stored, and
    /// announces it, storing nothing; whether any did. The server asks
    /// every second, so an open review of the plugin can offer to reload.
    pub fn reload_if_links_changed(&self) -> Result<bool, Error> {
        if !self.registry.notice_links()? {
            return Ok(false);
        }
        self.announce()?;
        Ok(true)
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
    /// row, as the listing shows it, with the version it replaced as
    /// `replaced_version`, and whether that one was newer as `older`.
    pub async fn install(&self, source: &str, options: InstallOptions) -> Result<Value, Error> {
        let worker = self.clone();
        let source = source.to_string();
        let installed = tokio::task::spawn_blocking(move || {
            install::install(&worker.db, &worker.registry, &source, options)
        })
        .await
        .map_err(|error| Error::Internal(error.to_string()))?;
        self.installed(installed)
    }

    /// Installs the official plugin `id` names, or updates it to the
    /// highest version a catalog offers; the plugin's row, as
    /// [`PluginService::install`] answers.
    pub async fn install_offered(&self, id: &str) -> Result<Value, Error> {
        let worker = self.clone();
        let id = id.to_string();
        let installed = tokio::task::spawn_blocking(move || {
            install::install_offered(&worker.db, &worker.registry, &id)
        })
        .await
        .map_err(|error| Error::Internal(error.to_string()))?;
        self.installed(installed)
    }

    /// An install's answer: the plugin's row, with the version it replaced.
    fn installed(
        &self,
        installed: Result<(crate::db::InstallRecord, Option<String>), Error>,
    ) -> Result<Value, Error> {
        let (record, before) = installed?;
        self.announce()?;
        let mut row = self
            .registry
            .get(&record.name)
            .map(|p| p.to_json())
            .ok_or_else(|| {
                Error::Internal(format!(
                    "{} was installed and is not registered",
                    record.name
                ))
            })?;
        let older = before.as_deref().is_some_and(|before| {
            install::semver(row["version"].as_str().unwrap_or_default()) < install::semver(before)
        });
        row["replaced_version"] = before.into();
        row["older"] = older.into();
        Ok(row)
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
