//! Plugin operations shared by the HTTP API and other application interfaces.

use std::sync::Arc;

use serde_json::{Value, json};

use super::jobs::Jobs;
use super::{InstallJob, InstallOptions, Plugin, Registry, install};
use crate::db::{Db, InstalledRecord};
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

    /// The sample a usable plugin ships, or why there is none to send.
    pub fn sample(&self, name: &str) -> Result<super::Sample, Error> {
        let plugin = self
            .registry
            .all()
            .into_iter()
            .find(|p| p.name == name && p.usable())
            .ok_or_else(|| {
                Error::invalid("/plugin", format!("no usable plugin is named {name}"))
            })?;
        match (&plugin.sample, &plugin.sample_error) {
            (Some(sample), _) => Ok(sample.clone()),
            (None, Some(why)) => Err(Error::invalid(
                "/sample",
                format!("{name}'s sample does not load: {why}"),
            )),
            (None, None) => Err(Error::invalid("/sample", format!("{name} has no sample"))),
        }
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
        json!({ "plugins": plugins })
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
            return Err(Error::NotFound(format!("plugin {name}")));
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
        let answer = install::remove(&self.db, &self.registry, name)?;
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
                    .get(&record.name)
                    .map(|p| p.to_json())
                    .ok_or_else(|| {
                        Error::Internal(format!(
                            "{} was installed and is not registered",
                            record.name
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

    /// Updates from the installation's original source. Links and pinned versions
    /// are refused; unchanged sources do not start a job or announce a change.
    pub async fn start_update(&self, name: &str) -> Result<UpdateOutcome, Error> {
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
                return Ok(UpdateOutcome::UpToDate {
                    version: record.version,
                });
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
        Ok(UpdateOutcome::Started {
            job_id: self.start_install(&source, options),
        })
    }

    fn installed(&self, name: &str) -> Result<InstalledRecord, Error> {
        self.db
            .installed_plugins()?
            .into_iter()
            .find(|r| r.name == name)
            .ok_or_else(|| Error::NotFound(format!("plugin {name}")))
    }

    async fn check_record(&self, record: InstalledRecord) -> Result<Value, Error> {
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
