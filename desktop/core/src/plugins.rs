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
    /// The plugin's own settings, as the manifest declares them: the
    /// resolved schema document, its compiled form, or why it was dropped.
    pub settings_schema: Option<Value>,
    settings_validator: Option<Schema>,
    pub settings_error: Option<String>,
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
                settings_schema: None,
                settings_validator: None,
                settings_error: None,
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
        // a bad settings schema costs the plugin its settings, not its place
        let (settings_schema, settings_validator, settings_error) =
            match manifest.get("settings_schema") {
                None | Some(Value::Null) => (None, None, None),
                Some(raw) => match settings::load(dir, &name, version, raw) {
                    Ok((document, validator)) => (Some(document), Some(validator), None),
                    Err(message) => (None, None, Some(message)),
                },
            };

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
            settings_schema,
            settings_validator,
            settings_error,
            error: None,
        })
    }

    pub fn usable(&self) -> bool {
        self.error.is_none()
    }

    /// Whether the manifest declares settings the core accepted.
    pub fn has_settings(&self) -> bool {
        self.settings_validator.is_some()
    }

    /// A change to the plugin's settings checked against its schema; the
    /// paths come back under `/plugins/<name>`.
    pub fn validate_settings(&self, patch: &Value) -> Vec<Violation> {
        let prefix = format!("/plugins/{}", self.name);
        let Some(schema) = &self.settings_validator else {
            return vec![Violation::new(&prefix, "the plugin has no settings")];
        };
        if !patch.is_object() {
            return vec![Violation::new(&prefix, "must be a JSON object")];
        }
        schema
            .validate(patch)
            .into_iter()
            .map(|v| {
                // a property with choices names them, not the schema keyword
                let message = self
                    .setting_choices(v.path.trim_start_matches('/'))
                    .map(|choices| format!("must be one of {}", choices.join(", ")))
                    .unwrap_or(v.message);
                Violation::new(format!("{prefix}{}", v.path), message)
            })
            .collect()
    }

    /// The values a string setting may take, when the schema lists them.
    fn setting_choices(&self, key: &str) -> Option<Vec<String>> {
        let property = self.settings_schema.as_ref()?.get("properties")?.get(key)?;
        let values: Vec<String> =
            if let Some(items) = property.get("enum").and_then(Value::as_array) {
                items.iter().map(Value::to_string).collect()
            } else if let Some(items) = property.get("oneOf").and_then(Value::as_array) {
                items
                    .iter()
                    .filter_map(|i| i.get("const"))
                    .map(Value::to_string)
                    .collect()
            } else {
                return None;
            };
        (!values.is_empty()).then_some(values)
    }

    /// The settings as they stand: every declared default, with the
    /// values stored for this plugin over them. Keys the schema does not
    /// know are left out.
    pub fn effective_settings(&self, stored: &Value) -> Value {
        let mut out = Map::new();
        if let Some(properties) = self
            .settings_schema
            .as_ref()
            .and_then(|s| s.get("properties"))
            .and_then(Value::as_object)
        {
            for (key, property) in properties {
                let value = stored
                    .get(key)
                    .or_else(|| property.get("default"))
                    .cloned()
                    .unwrap_or(Value::Null);
                out.insert(key.clone(), value);
            }
        }
        Value::Object(out)
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
            "settings_schema": self.settings_schema,
            "settings_error": self.settings_error,
        })
    }
}

/// The settings a plugin declares: a flat object schema, every property a
/// boolean, string, integer or number with a default.
mod settings {
    use std::path::Path;

    use serde_json::Value;

    use crate::schema::{Schema, safe_join};

    const TYPES: &[&str] = &["boolean", "string", "integer", "number"];

    /// Resolves a top-level `$ref` to a file in the plugin directory, checks
    /// the shape, and compiles the document with unknown keys refused.
    pub fn load(
        dir: &Path,
        name: &str,
        version: u32,
        raw: &Value,
    ) -> Result<(Value, Schema), String> {
        let document = resolve(dir, raw)?;
        let Value::Object(map) = &document else {
            return Err("settings_schema must be a JSON Schema object".into());
        };
        if let Some(t) = map.get("type")
            && t != "object"
        {
            return Err("settings_schema must describe an object".into());
        }
        let Some(Value::Object(properties)) = map.get("properties") else {
            return Err("settings_schema must have properties".into());
        };
        for (key, property) in properties {
            check_property(key, property)?;
        }
        let mut root = map.clone();
        root.insert("type".into(), Value::String("object".into()));
        root.insert("additionalProperties".into(), Value::Bool(false));
        let validator =
            Schema::compile(dir, name, version, "settings_schema", &Value::Object(root))?;
        Ok((Value::Object(map.clone()), validator))
    }

    fn resolve(dir: &Path, raw: &Value) -> Result<Value, String> {
        let Value::Object(map) = raw else {
            return Err("settings_schema must be a JSON Schema object".into());
        };
        let Some(reference) = map.get("$ref") else {
            return Ok(raw.clone());
        };
        let Some(relative) = reference.as_str() else {
            return Err("settings_schema $ref must be a relative path".into());
        };
        let path = safe_join(dir, relative).ok_or_else(|| {
            format!("settings_schema $ref {relative} leaves the plugin directory")
        })?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("settings_schema $ref {relative} cannot be read ({e})"))?;
        serde_json::from_str(&text)
            .map_err(|e| format!("settings_schema $ref {relative} is not valid JSON ({e})"))
    }

    fn check_property(key: &str, property: &Value) -> Result<(), String> {
        let Value::Object(p) = property else {
            return Err(format!("settings_schema property {key} must be an object"));
        };
        let kind = match p.get("type").and_then(Value::as_str) {
            Some(t) if TYPES.contains(&t) => t,
            _ => {
                return Err(format!(
                    "settings_schema property {key} must have a type of {}",
                    TYPES.join(", ")
                ));
            }
        };
        let Some(default) = p.get("default") else {
            return Err(format!("settings_schema property {key} needs a default"));
        };
        if !fits(kind, default) {
            return Err(format!(
                "settings_schema property {key}: the default is not a {kind}"
            ));
        }
        if let Some(options) = p.get("enum") {
            let ok = options
                .as_array()
                .is_some_and(|items| !items.is_empty() && items.iter().all(|i| fits(kind, i)));
            if !ok {
                return Err(format!(
                    "settings_schema property {key}: enum must list {kind} values"
                ));
            }
        }
        if let Some(choices) = p.get("oneOf") {
            let ok = choices.as_array().is_some_and(|items| {
                !items.is_empty()
                    && items
                        .iter()
                        .all(|i| i.get("const").is_some_and(|c| fits(kind, c)))
            });
            if !ok {
                return Err(format!(
                    "settings_schema property {key}: oneOf must list {{\"const\": …}} {kind} values"
                ));
            }
        }
        Ok(())
    }

    fn fits(kind: &str, value: &Value) -> bool {
        match kind {
            "boolean" => value.is_boolean(),
            "string" => value.is_string(),
            "integer" => value.is_i64() || value.is_u64(),
            "number" => value.is_number(),
            _ => false,
        }
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

    /// A plugin directory with the given manifest fields on top of the
    /// minimum, and an empty view.
    fn plugin_dir(root: &Path, name: &str, extra: &str) -> PathBuf {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "<html></html>").unwrap();
        std::fs::write(
            dir.join("manifest.json"),
            format!(
                "{{\"name\":\"{name}\",\"version\":1,\"payload_schema\":{{}},\"decision_schema\":{{}}{extra}}}"
            ),
        )
        .unwrap();
        dir
    }

    const KNOBS: &str = r#","settings_schema":{"type":"object","properties":{
        "diff":{"type":"string","title":"Diff","enum":["inline","split"],"default":"inline"},
        "wrap":{"type":"boolean","default":true},
        "context":{"type":"integer","minimum":0,"maximum":20,"default":3}}}"#;

    #[test]
    fn a_settings_schema_gives_defaults_and_checks_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Plugin::load(&plugin_dir(tmp.path(), "knobs", KNOBS));
        assert!(p.usable() && p.has_settings(), "{:?}", p.error);
        assert_eq!(p.settings_error, None);
        assert_eq!(
            p.effective_settings(&Value::Null),
            serde_json::json!({"diff": "inline", "wrap": true, "context": 3})
        );
        assert_eq!(
            p.effective_settings(&serde_json::json!({"diff": "split", "stale": 1})),
            serde_json::json!({"diff": "split", "wrap": true, "context": 3}),
            "stored values win; keys the schema does not know are left out"
        );
        assert!(
            p.validate_settings(&serde_json::json!({"diff": "split"}))
                .is_empty()
        );
        let bad =
            p.validate_settings(&serde_json::json!({"diff": "wide", "context": 99, "nope": 1}));
        let diff = bad
            .iter()
            .find(|v| v.path == "/plugins/knobs/diff")
            .unwrap();
        assert_eq!(diff.message, "must be one of \"inline\", \"split\"");
        let paths: Vec<&str> = bad.iter().map(|v| v.path.as_str()).collect();
        assert!(paths.contains(&"/plugins/knobs/diff"), "{bad:?}");
        assert!(paths.contains(&"/plugins/knobs/context"), "{bad:?}");
        assert!(paths.contains(&"/plugins/knobs/nope"), "{bad:?}");
        // the row carries the schema for the rows in Settings
        assert_eq!(
            p.to_json()["settings_schema"]["properties"]["diff"]["title"],
            "Diff"
        );
    }

    #[test]
    fn a_bad_settings_schema_costs_only_the_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let cases = [
            (
                r#","settings_schema":{"properties":{"a":{"type":"string"}}}"#,
                "needs a default",
            ),
            (
                r#","settings_schema":{"properties":{"a":{"type":"object","default":{}}}}"#,
                "must have a type of",
            ),
            (
                r#","settings_schema":{"properties":{"a":{"type":"integer","default":"3"}}}"#,
                "the default is not a integer",
            ),
            (
                r#","settings_schema":{"type":"array"}"#,
                "must describe an object",
            ),
            (
                r#","settings_schema":{"$ref":"../outside.json"}"#,
                "leaves the plugin directory",
            ),
        ];
        for (i, (extra, expected)) in cases.iter().enumerate() {
            let p = Plugin::load(&plugin_dir(tmp.path(), &format!("p{i}"), extra));
            assert!(p.usable(), "{extra}: {:?}", p.error);
            assert!(!p.has_settings());
            let why = p.settings_error.clone().unwrap_or_default();
            assert!(why.contains(expected), "{extra}: {why}");
            assert!(p.to_json()["settings_schema"].is_null());
            assert_eq!(p.to_json()["settings_error"], why);
        }
    }

    #[test]
    fn a_settings_schema_may_live_in_its_own_file() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = plugin_dir(
            tmp.path(),
            "filed",
            r#","settings_schema":{"$ref":"settings.schema.json"}"#,
        );
        std::fs::write(
            dir.join("settings.schema.json"),
            r#"{"type":"object","properties":{"wrap":{"type":"boolean","default":false}}}"#,
        )
        .unwrap();
        let p = Plugin::load(&dir);
        assert!(p.has_settings(), "{:?}", p.settings_error);
        assert_eq!(
            p.effective_settings(&Value::Null),
            serde_json::json!({"wrap": false})
        );
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
