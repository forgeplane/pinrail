//! Plugins: discovery, the compiled schemas, and version snapshots.
//!
//! A plugin is a directory with a `manifest.json`, two schemas and a view.
//! One plugin defines one sort of review. History must render what was
//! shown: the first review submitted under a plugin version copies the
//! directory into `<data dir>/plugins/<name>/<version>/`, and reviews keep
//! validating and rendering from that copy after the live plugin moves on. A
//! manifest with `"dev": true` is served live and never snapshotted.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use include_dir::{Dir, include_dir};
use serde_json::{Map, Value};

use crate::error::{Error, Violation};
use crate::schema::Schema;

/// The plugin every server has, embedded from the reference server's tree.
static BUILTIN_LIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../server/priv/plugins/list");

const MANIFEST: &str = "manifest.json";

#[derive(Debug)]
pub struct Plugin {
    pub name: String,
    pub version: u32,
    pub title: String,
    pub path: PathBuf,
    pub entry: String,
    pub min_height: u32,
    pub dev: bool,
    pub editorial: bool,
    /// A lucide icon name, shown wherever the plugin is named.
    pub icon: Option<String>,
    pub manifest: Map<String, Value>,
    pub payload_schema: Option<Schema>,
    pub decision_schema: Option<Schema>,
    /// Set when the plugin could not be loaded; it is listed but unusable.
    pub error: Option<String>,
}

impl Plugin {
    /// Loads the plugin at `dir`. Never fails: a bad plugin comes back with `error`.
    pub fn load(dir: &Path) -> Plugin {
        match Self::try_load(dir) {
            Ok(plugin) => plugin,
            Err(message) => Plugin {
                name: dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                version: 0,
                title: String::new(),
                path: dir.to_path_buf(),
                entry: "index.html".into(),
                min_height: 400,
                dev: false,
                editorial: false,
                icon: None,
                manifest: Map::new(),
                payload_schema: None,
                decision_schema: None,
                error: Some(message),
            },
        }
    }

    fn try_load(dir: &Path) -> Result<Plugin, String> {
        let body = std::fs::read_to_string(dir.join(MANIFEST))
            .map_err(|e| format!("cannot read {MANIFEST} ({e})"))?;
        let manifest: Value = serde_json::from_str(&body)
            .map_err(|e| format!("{MANIFEST} is not valid JSON ({e})"))?;
        let Value::Object(manifest) = manifest else {
            return Err(format!("{MANIFEST} must be a JSON object"));
        };

        let name = string_field(&manifest, "name")?;
        if !valid_name(&name) {
            return Err(format!("name {name:?} is not valid"));
        }
        let version = match manifest.get("version") {
            Some(Value::Number(n)) if n.as_u64().is_some_and(|v| v > 0) => {
                n.as_u64().unwrap() as u32
            }
            _ => return Err("version is required and must be a positive integer".into()),
        };
        let entry = match manifest.get("entry") {
            None => "index.html".to_string(),
            Some(Value::String(s)) if !s.is_empty() && !s.starts_with('/') && !s.contains('\0') => {
                s.clone()
            }
            Some(other) => return Err(format!("entry {other} is not valid")),
        };
        for key in ["payload_schema", "decision_schema"] {
            if !manifest.contains_key(key) {
                return Err(format!("{key} is required"));
            }
        }
        if !dir.join(&entry).is_file() {
            return Err(format!("entry {entry} not found"));
        }
        let icon = match manifest.get("icon") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if valid_icon(s) => Some(s.clone()),
            Some(other) => {
                return Err(format!(
                    "icon {other} is not valid: a lucide icon name, like \"mail\" or \"git-pull-request\""
                ));
            }
        };

        let payload_schema = Schema::compile(
            dir,
            &name,
            version,
            "payload_schema",
            &manifest["payload_schema"],
        )?;
        let decision_schema = Schema::compile(
            dir,
            &name,
            version,
            "decision_schema",
            &manifest["decision_schema"],
        )?;

        Ok(Plugin {
            title: manifest
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| name.clone()),
            name,
            version,
            path: dir.to_path_buf(),
            entry,
            min_height: manifest
                .get("min_height")
                .and_then(Value::as_u64)
                .filter(|n| *n > 0)
                .map(|n| n as u32)
                .unwrap_or(400),
            dev: manifest.get("dev") == Some(&Value::Bool(true)),
            editorial: manifest.get("editorial") == Some(&Value::Bool(true)),
            icon,
            manifest,
            payload_schema: Some(payload_schema),
            decision_schema: Some(decision_schema),
            error: None,
        })
    }

    pub fn usable(&self) -> bool {
        self.error.is_none()
    }

    pub fn validate_payload(&self, payload: &Value) -> Vec<Violation> {
        Self::validate(self.payload_schema.as_ref(), payload)
    }

    pub fn validate_decision(&self, decision: &Value) -> Vec<Violation> {
        Self::validate(self.decision_schema.as_ref(), decision)
    }

    fn validate(schema: Option<&Schema>, data: &Value) -> Vec<Violation> {
        if !data.is_object() {
            return vec![Violation::new("", "must be a JSON object")];
        }
        schema.map(|s| s.validate(data)).unwrap_or_default()
    }

    /// What the API lists for a plugin.
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "name": self.name,
            "version": self.version,
            "title": self.title,
            "path": self.path.display().to_string(),
            "entry": self.entry,
            "min_height": self.min_height,
            "dev": self.dev,
            "editorial": self.editorial,
            "icon": self.icon,
            "usable": self.usable(),
            "error": self.error,
            "payload_schema": self.manifest.get("payload_schema"),
            "decision_schema": self.manifest.get("decision_schema"),
        })
    }
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Lucide names an icon in lowercase words joined by dashes.
fn valid_icon(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn string_field(manifest: &Map<String, Value>, key: &str) -> Result<String, String> {
    match manifest.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        _ => Err(format!("{key} is required and must be a string")),
    }
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
    snapshots: HashMap<(String, u32), Arc<Plugin>>,
    added_dirs: Vec<PathBuf>,
}

/// The registered plugins, and the snapshots of versions still in use.
#[derive(Debug)]
pub struct Registry {
    default_dirs: Vec<PathBuf>,
    snapshots_dir: PathBuf,
    state: RwLock<RegistryState>,
}

impl Registry {
    /// Scans `default_dirs` and `added_dirs`. Fails when two plugins share a name.
    pub fn open(
        default_dirs: Vec<PathBuf>,
        added_dirs: Vec<PathBuf>,
        snapshots_dir: PathBuf,
    ) -> Result<Registry, String> {
        let registry = Registry {
            default_dirs,
            snapshots_dir,
            state: RwLock::new(RegistryState {
                plugins: BTreeMap::new(),
                snapshots: HashMap::new(),
                added_dirs,
            }),
        };
        registry.reload()?;
        Ok(registry)
    }

    pub fn dirs(&self) -> Vec<PathBuf> {
        let state = self.state.read().unwrap();
        let mut dirs = self.default_dirs.clone();
        for dir in &state.added_dirs {
            if !dirs.contains(dir) {
                dirs.push(dir.clone());
            }
        }
        dirs
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

    /// The plugin at the version a review was submitted under: the current
    /// one when the version matches, else the snapshot.
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
            .snapshots
            .get(&(name.to_string(), version))
        {
            return Ok(p.clone());
        }
        let dir = self.snapshot_dir(name, version);
        if dir.join(MANIFEST).is_file() {
            let plugin = Plugin::load(&dir);
            if plugin.version == version && plugin.usable() {
                let plugin = Arc::new(plugin);
                self.state
                    .write()
                    .unwrap()
                    .snapshots
                    .insert((name.to_string(), version), plugin.clone());
                return Ok(plugin);
            }
        }
        Err(Error::invalid(
            "/plugin",
            format!("plugin {name} version {version} is not available"),
        ))
    }

    /// Rescans every directory. On a duplicate name the old state is kept.
    pub fn reload(&self) -> Result<usize, String> {
        let dirs = self.dirs();
        let mut loaded: Vec<Plugin> = Vec::new();
        for dir in &dirs {
            for sub in subdirs(dir) {
                loaded.push(Plugin::load(&sub));
            }
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
        state.snapshots.clear();
        Ok(state.plugins.len())
    }

    /// Registers a directory whose subdirectories are plugins, then reloads.
    /// On failure the directory is not kept.
    pub fn add_dir(&self, dir: &Path) -> Result<usize, String> {
        if !dir.is_dir() {
            return Err(format!("{} is not a directory", dir.display()));
        }
        let dir = std::path::absolute(dir).map_err(|e| e.to_string())?;
        if self.dirs().contains(&dir) {
            return self.reload();
        }
        self.state.write().unwrap().added_dirs.push(dir.clone());
        match self.reload() {
            Ok(n) => Ok(n),
            Err(message) => {
                self.state.write().unwrap().added_dirs.retain(|d| d != &dir);
                let _ = self.reload();
                Err(message)
            }
        }
    }

    pub fn added_dirs(&self) -> Vec<PathBuf> {
        self.state.read().unwrap().added_dirs.clone()
    }

    pub fn snapshot_dir(&self, name: &str, version: u32) -> PathBuf {
        self.snapshots_dir.join(name).join(version.to_string())
    }

    /// Makes sure the plugin's version is snapshotted, unless it is a dev
    /// plugin. Returns the directory its bundle is served from.
    pub fn ensure_snapshot(&self, plugin: &Plugin) -> std::io::Result<PathBuf> {
        if plugin.dev {
            return Ok(plugin.path.clone());
        }
        let dest = self.snapshot_dir(&plugin.name, plugin.version);
        if dest.join(MANIFEST).is_file() {
            return Ok(dest);
        }
        let tmp = dest.with_extension("tmp");
        let _ = std::fs::remove_dir_all(&tmp);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        copy_dir(&plugin.path, &tmp)?;
        std::fs::rename(&tmp, &dest)?;
        Ok(dest)
    }

    /// The directory a plugin's bundle is served from: the live directory
    /// for a plugin in development, the snapshot otherwise, taken now if it
    /// is missing.
    pub fn bundle_dir(&self, plugin: &Plugin) -> PathBuf {
        if plugin.dev {
            plugin.path.clone()
        } else {
            self.ensure_snapshot(plugin)
                .unwrap_or_else(|_| self.snapshot_dir(&plugin.name, plugin.version))
        }
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

/// Copies a plugin directory for a snapshot: what the view is served from,
/// not the sources it was built from.
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "node_modules" || name.to_string_lossy().starts_with('.') {
            continue;
        }
        let target = to.join(name);
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_names_are_lucide_names() {
        for ok in ["mail", "git-pull-request", "list-checks", "a1"] {
            assert!(super::valid_icon(ok), "{ok}");
        }
        for bad in [
            "",
            "Mail",
            "git_pull",
            "-mail",
            "mail-",
            "git--pull",
            "mail icon",
        ] {
            assert!(!super::valid_icon(bad), "{bad}");
        }
    }

    use super::*;

    fn registry(tmp: &Path) -> Registry {
        let builtin = install_builtin(&tmp.join("builtin")).unwrap();
        Registry::open(vec![builtin], vec![], tmp.join("plugins")).unwrap()
    }

    #[test]
    fn the_builtin_list_plugin_loads_and_snapshots() {
        let tmp = tempfile::tempdir().unwrap();
        let r = registry(tmp.path());
        let list = r.fetch("list").unwrap();
        assert_eq!((list.version, list.title.as_str()), (1, "List"));
        assert!(r.fetch("nope").is_err());
        let dir = r.ensure_snapshot(&list).unwrap();
        assert!(dir.join("manifest.json").is_file());
        assert!(dir.ends_with("plugins/list/1"));
        assert!(r.fetch_version("list", 1).is_ok());
        assert!(r.fetch_version("list", 9).is_err());
    }

    #[test]
    fn a_broken_plugin_is_listed_with_its_error() {
        let tmp = tempfile::tempdir().unwrap();
        let bad = tmp.path().join("user").join("broken");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("manifest.json"), "{\"name\":\"broken\"}").unwrap();
        let r = Registry::open(
            vec![tmp.path().join("user")],
            vec![],
            tmp.path().join("plugins"),
        )
        .unwrap();
        let broken = r.get("broken").unwrap();
        assert_eq!(
            broken.error.as_deref(),
            Some("version is required and must be a positive integer")
        );
        assert!(r.fetch("broken").is_err());
    }

    #[test]
    fn duplicate_names_are_refused_and_the_dir_is_not_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let r = registry(tmp.path());
        let other = tmp.path().join("other");
        install_builtin(&other).unwrap();
        let error = r.add_dir(&other).unwrap_err();
        assert!(error.contains("plugin list is defined at"), "{error}");
        assert_eq!(r.dirs().len(), 1);
        assert!(r.fetch("list").is_ok());
    }
}
