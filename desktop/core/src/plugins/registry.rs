//! Plugin discovery, installed versions and entries retained for review history.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use std::sync::{Arc, RwLock};

use include_dir::{Dir, include_dir};
use serde_json::Value;

use super::manifest::{Install, MANIFEST, Plugin};
use crate::db::InstalledRecord;
use crate::error::Error;

/// The plugin every server has, embedded from `builtin/list`.
static BUILTIN_LIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/builtin/list");

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

/// Writes the embedded plugin into `dir/list` and returns `dir`.
pub fn install_builtin(dir: &Path) -> std::io::Result<PathBuf> {
    let target = dir.join("list");
    std::fs::create_dir_all(&target)?;
    for file in BUILTIN_LIST.files() {
        std::fs::write(target.join(file.path()), file.contents())?;
    }
    Ok(dir.to_path_buf())
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
            state: RwLock::new(RegistryState {
                plugins: BTreeMap::new(),
                kept: HashMap::new(),
                records,
            }),
        };
        registry.reload()?;
        Ok(registry)
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
            if plugin.version == version && plugin.usable() {
                let plugin = Arc::new(plugin);
                self.state
                    .write()
                    .unwrap()
                    .kept
                    .insert((name.to_string(), version), plugin.clone());
                return Ok(plugin);
            }
        }
        Err(Error::invalid(
            "/plugin",
            format!("plugin {name} version {version} is not installed"),
        ))
    }

    /// Loads everything again with a new set of records. On a duplicate
    /// name the old state is kept.
    pub fn reload_with(&self, records: Vec<InstalledRecord>) -> Result<usize, String> {
        self.state.write().unwrap().records = records;
        self.reload()
    }

    /// Reads every plugin again: the default directories, the linked
    /// folders, the store entries — each of the last with its record and,
    /// for a store entry, its files hashed against what was installed. On
    /// a duplicate name the old state is kept.
    pub fn reload(&self) -> Result<usize, String> {
        let records = self.state.read().unwrap().records.clone();
        let mut loaded: Vec<Plugin> = Vec::new();
        for dir in [&self.builtin_dir] {
            for sub in subdirs(dir) {
                loaded.push(Plugin::load(&sub));
            }
        }
        for record in &records {
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
            if plugin.error.is_none() && plugin.name != record.name {
                plugin.error = Some(format!(
                    "the manifest names {}, the record {}",
                    plugin.name, record.name
                ));
            }
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

    fn registry(tmp: &Path) -> Registry {
        let builtin = install_builtin(&tmp.join("builtin")).unwrap();
        Registry::open(builtin, vec![], tmp.join("store")).unwrap()
    }

    #[test]
    fn the_builtin_list_plugin_loads_and_is_fetched_by_version() {
        let tmp = tempfile::tempdir().unwrap();
        let r = registry(tmp.path());
        let list = r.fetch("list").unwrap();
        assert_eq!((list.version, list.title.as_str()), (1, "Action list"));
        assert!(r.fetch("nope").is_err());
        assert!(r.fetch_version("list", 1).is_ok());
        assert!(r.fetch_version("list", 9).is_err());
    }

    #[test]
    fn a_broken_plugin_is_listed_with_its_error() {
        let tmp = tempfile::tempdir().unwrap();
        let bad = tmp.path().join("user").join("broken");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("manifest.json"), "{\"name\":\"broken\"}").unwrap();
        let r = Registry::open(tmp.path().join("user"), vec![], tmp.path().join("store")).unwrap();
        let broken = r.get("broken").unwrap();
        assert_eq!(
            broken.error.as_deref(),
            Some("version is required: a positive integer, or a semantic version like \"1.2.0\"")
        );
        assert!(r.fetch("broken").is_err());
    }

    #[test]
    fn duplicate_names_are_refused_and_the_old_state_stands() {
        let tmp = tempfile::tempdir().unwrap();
        let r = registry(tmp.path());
        // a second plugin calling itself list, offered as a linked record
        let other = tmp.path().join("other");
        install_builtin(&other).unwrap();
        let path = other.join("list").display().to_string();
        let clash = InstalledRecord {
            name: "list".into(),
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
        };
        let error = r.reload_with(vec![clash]).unwrap_err();
        assert!(error.contains("plugin list is defined at"), "{error}");
        assert!(r.fetch("list").is_ok(), "the old state stands");
    }
}
