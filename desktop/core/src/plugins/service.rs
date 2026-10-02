//! Plugin operations shared by the HTTP API and other application interfaces.

use std::sync::Arc;

use serde_json::{Value, json};

use super::jobs::Jobs;
use super::{InstallJob, InstallOptions, Plugin, Registry, install};
use crate::db::{Db, InstallRecord};
use crate::error::Error;
use crate::events::{self, Bus, Notice};

#[derive(Debug, Clone)]
pub struct PluginService {
    db: Arc<Db>,
    registry: Arc<Registry>,
    bus: Bus,
    jobs: Arc<Jobs>,
}

/// Updating either starts a job or reports that the installed version is current.
#[derive(Debug, PartialEq, Eq)]
pub enum UpdateOutcome {
    UpToDate { version: String },
    Started { job_id: String },
}

impl PluginService {
    pub(crate) fn new(db: Arc<Db>, registry: Arc<Registry>, bus: Bus) -> Self {
        Self {
            db,
            registry,
            bus,
            jobs: Arc::new(Jobs::default()),
        }
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

    /// A plugin's lines: the one new reviews use, and each line kept for
    /// the reviews that render with it, with its current release. A removed
    /// plugin whose lines reviews still use is named in full.
    pub fn lines(&self, name: &str) -> Result<Value, Error> {
        let plugin = match self.registry.resolve(name) {
            Ok(plugin) => plugin,
            Err(_) if name.contains('/') => name.to_string(),
            Err(_) => return Err(Error::NotFound(format!("plugin {name}"))),
        };
        let mut lines = Vec::new();
        for line in self.db.lines()?.into_iter().filter(|l| l.plugin == plugin) {
            let version = self
                .db
                .bundle(&line.bundle)?
                .map(|b| b.version)
                .unwrap_or_default();
            lines.push(json!({ "line": line.line, "version": version, "bundle": line.bundle }));
        }
        let current = self.registry.get(&plugin).filter(|p| p.usable());
        if current.is_none() && lines.is_empty() {
            return Err(Error::NotFound(format!("plugin {name}")));
        }
        lines.sort_by_key(|l| pinrail_format::semver(l["version"].as_str().unwrap_or_default()));
        Ok(json!({
            "plugin": plugin,
            "current": current.as_ref().map(|p| p.line.clone()),
            "lines": lines,
        }))
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

    /// The plugin a review renders with: its line's current bundle, even
    /// after its plugin was removed.
    pub fn fetch_line(&self, plugin: &str, line: &str) -> Result<Arc<Plugin>, Error> {
        self.registry.fetch_line(plugin, line)
    }

    /// The bundle a line renders with.
    pub fn line_bundle(&self, plugin: &str, line: &str) -> Option<String> {
        self.registry.line_bundle(plugin, line)
    }

    /// Removes the installation while retaining versions still used by reviews.
    pub fn remove(&self, name: &str) -> Result<Value, Error> {
        let answer = install::remove(&self.db, &self.registry, name)?;
        self.announce()?;
        Ok(answer)
    }

    pub fn reload(&self) -> Result<usize, Error> {
        let count = self.registry.reload()?;
        self.announce()?;
        Ok(count)
    }

    /// Fetches and inspects a source without installing it or announcing a change.
    pub async fn inspect(&self, source: &str, options: InstallOptions) -> Result<Value, Error> {
        let worker = self.clone();
        let source = source.to_string();
        tokio::task::spawn_blocking(move || {
            install::inspect(&worker.db, &worker.registry, &source, options, &|_| {})
        })
        .await
        .map_err(|error| Error::Internal(error.to_string()))?
    }

    /// Starts an installation and returns the id used to follow its progress.
    /// Requires a Tokio runtime. Failures are recorded on the job.
    pub fn start_install(&self, source: &str, options: InstallOptions) -> String {
        let id = self.jobs.start(source);
        let job_id = id.clone();
        let worker = self.clone();
        let source = source.to_string();
        tokio::task::spawn_blocking(move || {
            let progress = |p| worker.jobs.note(&job_id, p);
            // a panic still ends the job, as failed: left running, it
            // would keep everyone who follows it waiting for good
            let installed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                install::install(&worker.db, &worker.registry, &source, options, &progress)
            }))
            .unwrap_or_else(|_| Err(Error::Internal("the install stopped unexpectedly".into())));
            let outcome = installed.and_then(|record| {
                worker.announce()?;
                worker
                    .registry
                    .get(&record.plugin)
                    .map(|p| p.to_json())
                    .ok_or_else(|| {
                        Error::Internal(format!(
                            "{} was installed and is not registered",
                            record.plugin
                        ))
                    })
            });
            worker.jobs.finish(&job_id, outcome);
        });
        id
    }

    /// A snapshot of an installation's progress or final result.
    pub fn job(&self, id: &str) -> Result<InstallJob, Error> {
        self.jobs
            .get(id)
            .ok_or_else(|| Error::NotFound(format!("install job {id}")))
    }

    /// Asks the original source whether a newer version is available.
    pub async fn check_updates(&self, name: &str) -> Result<Value, Error> {
        self.check_record(self.installed(name)?).await
    }

    /// What updating would install, without installing it: the inspection
    /// of the newer version, with `"state": "available"`, or
    /// `{"state": "up_to_date", "version"}`. An update that runs a build
    /// needs the `expect` this answers with.
    pub async fn inspect_update(&self, name: &str) -> Result<Value, Error> {
        match self.update_source(name).await? {
            Err(version) => Ok(serde_json::json!({ "state": "up_to_date", "version": version })),
            Ok((source, options)) => {
                let mut seen = self.inspect(&source, options).await?;
                seen["state"] = Value::String("available".into());
                Ok(seen)
            }
        }
    }

    /// Updates from the installation's original source, running a build
    /// only as `expect` confirms it. Links and pinned versions are refused;
    /// unchanged sources do not start a job or announce a change.
    pub async fn start_update(
        &self,
        name: &str,
        expect: Option<install::Expect>,
    ) -> Result<UpdateOutcome, Error> {
        match self.update_source(name).await? {
            Err(version) => Ok(UpdateOutcome::UpToDate { version }),
            Ok((source, mut options)) => {
                options.expect = expect;
                Ok(UpdateOutcome::Started {
                    job_id: self.start_install(&source, options),
                })
            }
        }
    }

    /// The source and options that update the plugin, or its version when
    /// it is up to date.
    async fn update_source(
        &self,
        name: &str,
    ) -> Result<Result<(String, InstallOptions), String>, Error> {
        let record = self.installed(name)?;
        let answer = self.check_record(record.clone()).await?;
        match answer["state"].as_str().unwrap_or("unknown") {
            "linked" => {
                return Err(Error::invalid(
                    "/name",
                    format!("{name} is a link: it is always what its folder holds"),
                ));
            }
            "pinned" => {
                let at = answer["tag"]
                    .as_str()
                    .or(answer["ref"].as_str())
                    .unwrap_or("this version");
                return Err(Error::invalid(
                    "/name",
                    format!("{name} is pinned to {at}; install another ref to move it"),
                ));
            }
            "up_to_date" => {
                let version = self.registry.get(&record.plugin).map(|p| p.version.clone());
                return Ok(Err(version.unwrap_or_default()));
            }
            _ => {}
        }
        let (mut source, mut options) = install::source_of(&record);
        // the release the check found, by its tag: a plugin whose tag names
        // it shares its repository's latest release with the app and with
        // the other plugins released there
        if record.kind == "release"
            && let Some(tag) = answer["tag"].as_str()
            && let Some(page) = source.strip_suffix("/releases")
        {
            source = format!("{page}/releases/tag/{tag}");
        }
        options.updates = Some(record.name.clone());
        Ok(Ok((source, options)))
    }

    fn installed(&self, name: &str) -> Result<InstallRecord, Error> {
        let plugin = self
            .registry
            .resolve(name)
            .map_err(|_| Error::NotFound(format!("plugin {name}")))?;
        let record = self
            .db
            .install(&plugin)?
            .ok_or_else(|| Error::NotFound(format!("plugin {name}")))?;
        if record.kind == "bundled" {
            return Err(Error::invalid(
                "/name",
                format!("{plugin} ships with Pinrail and is updated with it"),
            ));
        }
        Ok(record)
    }

    async fn check_record(&self, record: InstallRecord) -> Result<Value, Error> {
        let registry = self.registry.clone();
        tokio::task::spawn_blocking(move || install::check_updates(&registry, &record))
            .await
            .map_err(|error| Error::Internal(error.to_string()))
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
