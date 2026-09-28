//! Plugin discovery, installed versions and entries retained for review history.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use std::sync::{Arc, RwLock};

use include_dir::{Dir, include_dir};
use serde_json::Value;

use super::manifest::{Install, MANIFEST, Plugin};
use crate::db::InstalledRecord;
use crate::error::Error;

/// The plugins every server has. They live in `plugins/` with the others;
/// build.rs copies their bundles here for the binary to carry.
static BUILTIN: Dir = include_dir!("$OUT_DIR/builtin");

/// Whether a plugin of this name ships with the app, in which case it cannot
/// be installed over: the built-in copy is the one that is served.
pub fn is_builtin(name: &str) -> bool {
    BUILTIN.dirs().any(|d| d.path().to_string_lossy() == name)
}

/// SHA-256 over a directory's files: each relative path and its bytes, in
/// sorted order, with the same exclusions the copier applies.
pub(super) fn hash_dir(dir: &Path) -> std::io::Result<String> {
    hash_dir_where(dir, &|name| {
        name != "node_modules" && !name.starts_with('.')
    })
}

/// `hash_dir` over the entries `keep` admits, by name, at every level.
pub(super) fn hash_dir_where(dir: &Path, keep: &dyn Fn(&str) -> bool) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    fn walk(
        root: &Path,
        dir: &Path,
        keep: &dyn Fn(&str) -> bool,
        out: &mut Vec<PathBuf>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if !keep(&entry.file_name().to_string_lossy()) {
                continue;
            }
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                walk(root, &path, keep, out)?;
            } else {
                out.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(dir, dir, keep, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for relative in files {
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(std::fs::read(dir.join(&relative))?);
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Writes the embedded plugins into `dir`, one folder each, and returns `dir`.
/// Every start rewrites them, so an upgraded binary brings its own copies.
pub fn install_builtin(dir: &Path) -> std::io::Result<PathBuf> {
    for plugin in BUILTIN.dirs() {
        write_dir(plugin, dir)?;
    }
    Ok(dir.to_path_buf())
}

/// One embedded directory under `into`, with the files below it.
fn write_dir(source: &Dir, into: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(into.join(source.path()))?;
    for file in source.files() {
        std::fs::write(into.join(file.path()), file.contents())?;
    }
    for child in source.dirs() {
        write_dir(child, into)?;
    }
    Ok(())
}

#[derive(Debug)]
struct RegistryState {
    plugins: BTreeMap<String, Arc<Plugin>>,
    /// store entries a review still renders from, loaded on demand
    kept: HashMap<(String, u32), Arc<Plugin>>,
    /// the installed plugins, links and store entries, as last given
    records: Vec<InstalledRecord>,
}

/// The registered plugins — the built-in and configured directories, the
/// linked folders, the store — and the store entries kept for reviews.
#[derive(Debug)]
pub struct Registry {
    /// The plugin every install starts from, shipped inside the app.
    builtin_dir: PathBuf,
    store_dir: PathBuf,
    /// Where release installs and update checks ask GitHub.
    github_api: String,
    /// How long a plugin's build may run.
    build_timeout: std::time::Duration,
    state: RwLock<RegistryState>,
}

impl Registry {
    /// Loads the built-in plugin and the installed `records`.
    /// Fails when two plugins share a name.
    pub fn open(
        builtin_dir: PathBuf,
        records: Vec<InstalledRecord>,
        store_dir: PathBuf,
    ) -> Result<Registry, String> {
        let registry = Registry {
            builtin_dir,
            store_dir,
            github_api: "https://api.github.com".to_string(),
            build_timeout: crate::config::BUILD_TIMEOUT,
            state: RwLock::new(RegistryState {
                plugins: BTreeMap::new(),
                kept: HashMap::new(),
                records,
            }),
        };
        registry.reload()?;
        Ok(registry)
    }

    /// Asks another GitHub API root than the public one.
    pub fn with_github_api(mut self, root: impl Into<String>) -> Self {
        self.github_api = root.into();
        self
    }

    pub fn github_api(&self) -> &str {
        &self.github_api
    }

    /// Stops builds after `timeout` rather than the default.
    pub fn with_build_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.build_timeout = timeout;
        self
    }

    pub fn build_timeout(&self) -> std::time::Duration {
        self.build_timeout
    }

    pub fn store_dir(&self) -> &Path {
        &self.store_dir
    }

    /// Where a store entry lives: one per plugin and major.
    pub fn store_entry(&self, name: &str, major: i64) -> PathBuf {
        self.store_dir.join(name).join(major.to_string())
    }

    pub fn records(&self) -> Vec<InstalledRecord> {
        self.state.read().unwrap().records.clone()
    }

    pub fn all(&self) -> Vec<Arc<Plugin>> {
        self.state
            .read()
            .unwrap()
            .plugins
            .values()
            .cloned()
            .collect()
    }

    /// The current plugin, usable or not.
    pub fn get(&self, name: &str) -> Option<Arc<Plugin>> {
        self.state.read().unwrap().plugins.get(name).cloned()
    }

    /// The usable current plugin, or an `invalid` error pointing at `/plugin`.
    pub fn fetch(&self, name: &str) -> Result<Arc<Plugin>, Error> {
        match self.get(name) {
            Some(p) if p.usable() => Ok(p),
            Some(p) => Err(Error::invalid(
                "/plugin",
                format!(
                    "plugin {name} is not usable: {}",
                    p.error.clone().unwrap_or_default()
                ),
            )),
            None => Err(Error::invalid("/plugin", format!("unknown plugin {name}"))),
        }
    }

    /// The plugin at the version a review was created under: the current
    /// one when the version matches, else the store entry kept for the
    /// reviews that still render from it.
    pub fn fetch_version(&self, name: &str, version: u32) -> Result<Arc<Plugin>, Error> {
        let missing = || {
            Error::invalid(
                "/plugin",
                format!("plugin {name} version {version} is not installed"),
            )
        };
        // the name is joined into a store path below, and comes from a URL
        if !super::manifest::valid_name(name) {
            return Err(missing());
        }
        if let Some(p) = self.get(name)
            && p.version == version
            && p.usable()
        {
            return Ok(p);
        }
        if let Some(p) = self
            .state
            .read()
            .unwrap()
            .kept
            .get(&(name.to_string(), version))
        {
            return Ok(p.clone());
        }
        let dir = self.store_entry(name, version as i64);
        if dir.join(MANIFEST).is_file() {
            let plugin = Plugin::load(&dir);
            if plugin.name == name && plugin.version == version && plugin.usable() {
                let plugin = Arc::new(plugin);
                self.state
                    .write()
                    .unwrap()
                    .kept
                    .insert((name.to_string(), version), plugin.clone());
                return Ok(plugin);
            }
        }
        Err(missing())
    }

    /// Loads everything again with a new set of records. On a duplicate
    /// name the old state is kept, records and plugins alike.
    pub fn reload_with(&self, records: Vec<InstalledRecord>) -> Result<usize, String> {
        self.load(records)
    }

    /// Reads every plugin again: the built-in ones, the linked folders, the
    /// store entries — each of the last with its record and, for a store
    /// entry, its files hashed against what was installed. A record whose
    /// name is built in is skipped; on any other duplicate name the old
    /// state is kept.
    pub fn reload(&self) -> Result<usize, String> {
        let records = self.state.read().unwrap().records.clone();
        self.load(records)
    }

    /// Builds the state from `records` and, only when it is sound, puts the
    /// records and the plugins in place together.
    fn load(&self, records: Vec<InstalledRecord>) -> Result<usize, String> {
        let mut loaded: Vec<Plugin> = subdirs(&self.builtin_dir)
            .into_iter()
            .map(|sub| Plugin::load(&sub))
            .collect();
        for record in &records {
            // A plugin that has since become built-in: the copy in the binary
            // is the one served, and the record is left where it is rather
            // than failing the whole registry over a name it no longer owns.
            if is_builtin(&record.name) {
                continue;
            }
            let dir = if record.linked {
                PathBuf::from(&record.path)
            } else {
                self.store_entry(&record.name, record.major)
            };
            let mut plugin = Plugin::load(&dir);
            let modified = match (&record.hash, record.linked) {
                (Some(expected), false) => hash_dir(&dir).map(|h| &h != expected).unwrap_or(true),
                _ => false,
            };
            // Listed under the record's name, whatever the folder holds:
            // records are unique by name, so a broken or renamed plugin
            // costs only itself. A failed load names it after its folder,
            // which for a store entry is only its major.
            if plugin.error.is_none() && plugin.name != record.name {
                plugin.error = Some(format!(
                    "the manifest names {}, the record {}",
                    plugin.name, record.name
                ));
            }
            plugin.name = record.name.clone();
            plugin.install = Some(Install {
                kind: record.kind.clone(),
                source: record.source.clone(),
                version: record.version.clone(),
                linked: record.linked,
                commit: record.commit.clone(),
                tag: (record.kind == "release")
                    .then(|| serde_json::from_str::<Value>(&record.resolved).ok())
                    .flatten()
                    .and_then(|r| r["tag"].as_str().map(str::to_string)),
                asset_hash: record.asset_hash.clone(),
                hash: record.hash.clone(),
                modified,
                installed_at: record.installed_at.clone(),
            });
            loaded.push(plugin);
        }
        let mut by_name: BTreeMap<String, Vec<&Plugin>> = BTreeMap::new();
        for p in &loaded {
            by_name.entry(p.name.clone()).or_default().push(p);
        }
        let duplicates: Vec<String> = by_name
            .iter()
            .filter(|(_, ps)| ps.len() > 1)
            .map(|(name, ps)| {
                format!(
                    "plugin {name} is defined at {}",
                    ps.iter()
                        .map(|p| p.path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(" and ")
                )
            })
            .collect();
        if !duplicates.is_empty() {
            return Err(duplicates.join("; "));
        }
        let mut state = self.state.write().unwrap();
        state.records = records;
        state.plugins = loaded
            .into_iter()
            .map(|p| (p.name.clone(), Arc::new(p)))
            .collect();
        state.kept.clear();
        Ok(state.plugins.len())
    }
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut subs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(MANIFEST).is_file())
        .collect();
    subs.sort();
    subs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A linked record for a plugin folder, as an install would have written.
    fn linked(name: &str, dir: &Path) -> InstalledRecord {
        let path = dir.display().to_string();
        InstalledRecord {
            name: name.into(),
            version: "1.0.0".into(),
            major: 1,
            kind: "path".into(),
            source: path.clone(),
            resolved: path.clone(),
            commit: None,
            asset_hash: None,
            hash: None,
            build_log: None,
            installed_at: "2026-09-01T10:00:00Z".into(),
            linked: true,
            path,
        }
    }

    fn registry(tmp: &Path) -> Registry {
        let builtin = install_builtin(&tmp.join("builtin")).unwrap();
        Registry::open(builtin, vec![], tmp.join("store")).unwrap()
    }

    #[test]
    fn the_builtin_plugins_load_and_are_fetched_by_version() {
        let tmp = tempfile::tempdir().unwrap();
        let r = registry(tmp.path());
        let list = r.fetch("list").unwrap();
        assert_eq!((list.version, list.title.as_str()), (1, "Action list"));
        assert!(r.fetch("nope").is_err());
        assert!(r.fetch_version("list", 1).is_ok());
        assert!(r.fetch_version("list", 9).is_err());

        // the second built-in keeps its files in subdirectories: a view and
        // the schemas it refers to, which are written out with it
        let feedback = r.fetch("feedback").unwrap();
        assert_eq!(feedback.entry, "view/index.html");
        assert!(feedback.error.is_none(), "{:?}", feedback.error);
        assert!(feedback.path.join("view/feedback-core.js").is_file());
        assert!(feedback.path.join("schemas/payload.schema.json").is_file());
    }

    #[test]
    fn an_installed_plugin_that_became_builtin_is_shadowed_by_it() {
        let tmp = tempfile::tempdir().unwrap();
        let builtin = install_builtin(&tmp.path().join("builtin")).unwrap();
        // someone installed feedback from a folder before it shipped with
        // the app; two plugins of one name would otherwise refuse to load
        let elsewhere = tmp.path().join("elsewhere");
        install_builtin(&elsewhere).unwrap();
        let record = linked("feedback", &elsewhere.join("feedback"));
        let r = Registry::open(builtin.clone(), vec![record], tmp.path().join("store")).unwrap();

        // the copy in the binary is the one served, and the rest still loads
        assert!(is_builtin("feedback"));
        assert_eq!(r.fetch("feedback").unwrap().path, builtin.join("feedback"));
        assert!(r.fetch("feedback").unwrap().install.is_none());
        assert!(r.fetch("list").is_ok());
    }

    #[test]
    fn a_broken_plugin_is_listed_with_its_error() {
        let tmp = tempfile::tempdir().unwrap();
        let bad = tmp.path().join("user").join("broken");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("manifest.json"), "{\"name\":\"broken\"}").unwrap();
        let r = Registry::open(tmp.path().join("user"), vec![], tmp.path().join("store")).unwrap();
        let broken = r.get("broken").unwrap();
        // the manifest schema's first word on it: a required key is missing
        assert!(
            broken
                .error
                .as_deref()
                .is_some_and(|e| e.ends_with("is required")),
            "{:?}",
            broken.error
        );
        assert!(r.fetch("broken").is_err());
    }

    #[test]
    fn broken_store_entries_cost_only_themselves() {
        // two installed plugins at the same major whose store folders are
        // gone: a bad upgrade, a hand deletion, an interrupted install
        let tmp = tempfile::tempdir().unwrap();
        let builtin = install_builtin(&tmp.path().join("builtin")).unwrap();
        let store = tmp.path().join("store");
        let installed = |name: &str| InstalledRecord {
            kind: "git".into(),
            linked: false,
            path: store.join(name).join("1").display().to_string(),
            ..linked(name, &store.join(name).join("1"))
        };
        let r = Registry::open(builtin, vec![installed("alpha"), installed("beta")], store)
            .expect("one bad plugin must not stop the app from starting");
        for name in ["alpha", "beta"] {
            let plugin = r
                .get(name)
                .unwrap_or_else(|| panic!("{name} is listed under its own name"));
            assert!(plugin.error.is_some(), "{name} carries its error");
            assert!(r.fetch(name).is_err());
        }
        assert!(r.fetch("list").is_ok(), "the rest still work");
    }

    #[test]
    fn duplicate_names_are_refused_and_the_old_state_stands() {
        let tmp = tempfile::tempdir().unwrap();
        let r = registry(tmp.path());
        // two folders, each calling its plugin the same name, both linked
        let (one, two) = (tmp.path().join("one"), tmp.path().join("two"));
        for dir in [&one, &two] {
            install_builtin(dir).unwrap();
            let manifest = dir.join("list").join(MANIFEST);
            let text = std::fs::read_to_string(&manifest).unwrap();
            std::fs::write(&manifest, text.replace("\"list\"", "\"twin\"")).unwrap();
        }
        let records = vec![
            linked("twin", &one.join("list")),
            linked("twin", &two.join("list")),
        ];

        let error = r.reload_with(records).unwrap_err();
        assert!(error.contains("plugin twin is defined at"), "{error}");
        assert!(r.fetch("list").is_ok(), "the old state stands");
        assert!(
            r.records().is_empty(),
            "and so do the old records, which the history sweep reads"
        );
    }
}
