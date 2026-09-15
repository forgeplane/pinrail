//! Installing a plugin from a folder: inspect it, build it when its
//! manifest says so, place the bundle in the store, record where it came
//! from. A link points the registry at the folder instead and serves it
//! live.
//!
//! The store keeps one entry per plugin and major version, the latest
//! installed in that line: an equal or higher version replaces it, an
//! older one is refused unless forced. An old line stays while a review
//! still renders from it.
//!
//! An install is a job: it reports its step and its build log as it goes,
//! so a dialog or a terminal can follow a build that takes a minute.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use chrono::Utc;
use serde_json::{Map, Value};

use crate::db::{Db, InstalledRecord};
use crate::error::Error;
use crate::plugins::{Plugin, Registry, hash_dir};

pub struct Options {
    /// serve the folder live instead of copying it
    pub link: bool,
    /// replace a newer version already installed
    pub force: bool,
}

/// What an install says as it goes: the step it is at, and lines of the
/// build's output.
pub enum Progress {
    Step(&'static str),
    Log(String),
}

/// One install, followed by id: its step, its log, and how it ended.
#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub source: String,
    /// `fetching`, `inspecting`, `building`, `placing`, `done`, `failed`
    pub status: String,
    pub log: String,
    pub error: Option<String>,
    /// the plugin's row, once done
    pub plugin: Option<Value>,
}

impl Job {
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "id": self.id,
            "source": self.source,
            "status": self.status,
            "log": self.log,
            "error": self.error,
            "plugin": self.plugin,
        })
    }
}

/// The jobs the app has run, by id, for the dialog and the CLI to follow.
#[derive(Debug, Default)]
pub struct Jobs(Mutex<HashMap<String, Job>>);

impl Jobs {
    pub fn start(&self, source: &str) -> String {
        let id = crate::id::next().replace("r_", "j_");
        self.0.lock().unwrap().insert(
            id.clone(),
            Job {
                id: id.clone(),
                source: source.to_string(),
                status: "fetching".into(),
                log: String::new(),
                error: None,
                plugin: None,
            },
        );
        id
    }

    pub fn get(&self, id: &str) -> Option<Job> {
        self.0.lock().unwrap().get(id).cloned()
    }

    pub fn note(&self, id: &str, progress: Progress) {
        let mut jobs = self.0.lock().unwrap();
        let Some(job) = jobs.get_mut(id) else { return };
        match progress {
            Progress::Step(step) => job.status = step.to_string(),
            Progress::Log(line) => {
                job.log.push_str(&line);
                job.log.push('\n');
            }
        }
    }

    pub fn finish(&self, id: &str, outcome: Result<Value, Error>) {
        let mut jobs = self.0.lock().unwrap();
        let Some(job) = jobs.get_mut(id) else { return };
        match outcome {
            Ok(plugin) => {
                job.status = "done".into();
                job.plugin = Some(plugin);
            }
            Err(error) => {
                job.status = "failed".into();
                job.error = Some(error.to_string());
            }
        }
    }
}

/// Installs the plugin at `source`, telling `progress` as it goes. Returns
/// its record; the registry has been reloaded with it.
pub fn install_path(
    db: &Db,
    registry: &Registry,
    source: &Path,
    options: Options,
    progress: &dyn Fn(Progress),
) -> Result<InstalledRecord, Error> {
    progress(Progress::Step("inspecting"));
    let dir = std::path::absolute(source)?;
    if !dir.is_dir() {
        return Err(Error::invalid(
            "/source",
            format!("{} is not a directory", dir.display()),
        ));
    }
    let manifest = read_manifest(&dir)?;
    let build = build_command(&manifest)?;

    // a link serves the folder as it is; what the folder has to be, the
    // registry says when it loads it
    if options.link {
        let plugin = Plugin::load(&dir);
        if let Some(why) = &plugin.error {
            return Err(Error::invalid("/source", format!("not a plugin: {why}")));
        }
        let record = record_for(
            &plugin,
            source,
            &dir,
            None,
            None,
            true,
            dir.display().to_string(),
        );
        return commit(db, registry, record);
    }

    // build in a scratch copy, so the source is never written to
    let (staged, log_path) = match &build {
        Some(command) => {
            progress(Progress::Step("building"));
            let scratch = scratch_dir(registry, &manifest);
            copy_tree(&dir, &scratch, &[".git", "node_modules"])?;
            let log_path = run_build(registry, &manifest, &scratch, command, progress)?;
            (scratch, Some(log_path))
        }
        None => (dir.clone(), None),
    };
    let plugin = Plugin::load(&staged);
    if let Some(why) = &plugin.error {
        let _ = build.as_ref().map(|_| std::fs::remove_dir_all(&staged));
        return Err(Error::invalid(
            "/source",
            match build {
                Some(_) => format!("after the build, not a plugin: {why}"),
                None => format!(
                    "not a plugin: {why}; a source that needs building declares its build in the manifest"
                ),
            },
        ));
    }
    if let Some(refusal) = older_than_installed(db, &plugin, options.force)? {
        let _ = build.as_ref().map(|_| std::fs::remove_dir_all(&staged));
        return Err(refusal);
    }

    progress(Progress::Step("placing"));
    let entry = place(registry, &plugin, &staged)?;
    if build.is_some() {
        let _ = std::fs::remove_dir_all(&staged);
    }
    let hash = hash_dir(&entry)?;
    let record = record_for(
        &plugin,
        source,
        &dir,
        Some(hash),
        log_path.map(|p| p.display().to_string()),
        false,
        entry.display().to_string(),
    );
    commit(db, registry, record)
}

fn read_manifest(dir: &Path) -> Result<Map<String, Value>, Error> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).map_err(|e| {
        Error::invalid(
            "/source",
            format!("not a plugin: cannot read manifest.json ({e})"),
        )
    })?;
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(Error::invalid(
            "/source",
            "not a plugin: manifest.json is not a JSON object",
        )),
    }
}

/// The manifest's `build.command`, when it declares one.
fn build_command(manifest: &Map<String, Value>) -> Result<Option<String>, Error> {
    match manifest.get("build") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(build)) => match build.get("command").and_then(Value::as_str) {
            Some(command) if !command.trim().is_empty() => Ok(Some(command.trim().to_string())),
            _ => Err(Error::invalid(
                "/source",
                "the manifest's build needs a command",
            )),
        },
        Some(_) => Err(Error::invalid(
            "/source",
            "the manifest's build must be an object with a command",
        )),
    }
}

fn scratch_dir(registry: &Registry, manifest: &Map<String, Value>) -> PathBuf {
    let name = manifest
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("plugin");
    registry
        .store_dir()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| registry.store_dir().to_path_buf())
        .join("fetch")
        .join(format!(
            "{name}-{}",
            crate::id::next().trim_start_matches("r_")
        ))
}

/// Runs the build command through the shell in the scratch copy, its
/// output going to the log and to `progress` line by line. A non-zero exit
/// stops the install with the tail of the log.
fn run_build(
    registry: &Registry,
    manifest: &Map<String, Value>,
    scratch: &Path,
    command: &str,
    progress: &dyn Fn(Progress),
) -> Result<PathBuf, Error> {
    let name = manifest
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("plugin");
    let logs = registry
        .store_dir()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| registry.store_dir().to_path_buf())
        .join("logs");
    std::fs::create_dir_all(&logs)?;
    let log_path = logs.join(format!("{name}-{}.log", Utc::now().format("%Y%m%dT%H%M%S")));
    let mut log = std::fs::File::create(&log_path)?;
    use std::io::Write;
    writeln!(log, "$ {command}")?;
    progress(Progress::Log(format!("$ {command}")));

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(format!("{command} 2>&1"))
        .current_dir(scratch)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::invalid("/source", format!("the build could not start: {e}")))?;
    let mut tail: Vec<String> = Vec::new();
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines() {
            let line = line.unwrap_or_default();
            writeln!(log, "{line}")?;
            if tail.len() == 20 {
                tail.remove(0);
            }
            tail.push(line.clone());
            progress(Progress::Log(line));
        }
    }
    let status = child.wait()?;
    if !status.success() {
        return Err(Error::invalid(
            "/source",
            format!(
                "the build failed ({status}); the log is at {}\n{}",
                log_path.display(),
                tail.join("\n")
            ),
        ));
    }
    Ok(log_path)
}

/// Whether the same line already holds a newer version.
fn older_than_installed(db: &Db, plugin: &Plugin, force: bool) -> Result<Option<Error>, Error> {
    let records = db.installed_plugins()?;
    let current = records.iter().find(|r| r.name == plugin.name);
    if let Some(current) = current
        && !current.linked
        && current.major == plugin.version as i64
        && semver(&plugin.release) < semver(&current.version)
        && !force
    {
        return Ok(Some(Error::invalid(
            "/source",
            format!(
                "{} {} is older than the installed {}; pass force to replace it",
                plugin.name, plugin.release, current.version
            ),
        )));
    }
    Ok(None)
}

fn record_for(
    plugin: &Plugin,
    source: &Path,
    resolved: &Path,
    hash: Option<String>,
    build_log: Option<String>,
    linked: bool,
    path: String,
) -> InstalledRecord {
    InstalledRecord {
        name: plugin.name.clone(),
        version: plugin.release.clone(),
        major: plugin.version as i64,
        kind: "path".into(),
        source: source.display().to_string(),
        resolved: resolved.display().to_string(),
        commit: None,
        asset_hash: None,
        hash,
        build_log,
        installed_at: crate::review::iso(Utc::now()),
        linked,
        path,
    }
}

/// Writes the record, drops a previous line no review renders from, and
/// reloads the registry.
fn commit(db: &Db, registry: &Registry, record: InstalledRecord) -> Result<InstalledRecord, Error> {
    let previous = db
        .installed_plugins()?
        .into_iter()
        .find(|r| r.name == record.name);
    if let Some(previous) = previous
        && !previous.linked
        && (previous.major != record.major || record.linked)
        && !db.reviews_use(&previous.name, previous.major as u32)?
    {
        let _ = std::fs::remove_dir_all(registry.store_entry(&previous.name, previous.major));
    }
    db.upsert_installed(&record)?;
    let records = db.installed_plugins()?;
    registry
        .reload_with(records)
        .map_err(|message| Error::invalid("/source", message))?;
    Ok(record)
}

/// What never enters the store: sources and the tooling that builds them.
/// The bundle is what is left.
const NOT_IN_THE_BUNDLE: &[&str] = &[
    "node_modules",
    "src",
    "tests",
    "test",
    "fixtures",
    "package.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lockb",
    "tsconfig.json",
];

fn in_the_bundle(name: &str) -> bool {
    !name.starts_with('.')
        && !NOT_IN_THE_BUNDLE.contains(&name)
        && !name.starts_with("tsconfig.")
        && !name.starts_with("vite.config.")
        && !name.starts_with("vitest.config.")
        && !name.starts_with("playwright.config.")
}

/// Copies the plugin's bundle into the store entry for its line, whole or
/// not at all: the copy lands beside the entry and takes its place with
/// one rename.
fn place(registry: &Registry, plugin: &Plugin, dir: &Path) -> Result<PathBuf, Error> {
    let entry = registry.store_entry(&plugin.name, plugin.version as i64);
    let staging = entry.with_extension("staging");
    let _ = std::fs::remove_dir_all(&staging);
    if let Some(parent) = entry.parent() {
        std::fs::create_dir_all(parent)?;
    }
    copy_bundle(dir, &staging)?;
    if entry.exists() {
        let old = entry.with_extension("old");
        let _ = std::fs::remove_dir_all(&old);
        std::fs::rename(&entry, &old)?;
        std::fs::rename(&staging, &entry)?;
        let _ = std::fs::remove_dir_all(&old);
    } else {
        std::fs::rename(&staging, &entry)?;
    }
    Ok(entry)
}

fn copy_bundle(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if !in_the_bundle(&name.to_string_lossy()) {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_bundle(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// A copy of a source tree for building in, without the names given.
fn copy_tree(from: &Path, to: &Path, skip: &[&str]) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if skip.contains(&name.to_string_lossy().as_ref()) {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target, skip)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// "1.2.3" as something that orders; anything else sorts first.
pub fn semver(text: &str) -> (u64, u64, u64) {
    let mut parts = text.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}
