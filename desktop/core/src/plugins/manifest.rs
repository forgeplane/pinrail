//! Loading plugin manifests and validating their schemas, settings and shortcuts.
//! Invalid definitions remain inspectable as plugins carrying an error.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::error::Violation;
use crate::schema::Schema;

/// Every key a plugin's manifest may set, as the docs' manifest reference
/// lists it: the key, whether it is required, what it holds and what it
/// does. `load` reads these, and `build` is read by an install.
#[cfg(any(test, feature = "docs"))]
pub const KEYS: &[(&str, bool, &str, &str)] = &[
    (
        "name",
        true,
        "a string, `[a-z][a-z0-9_-]*`",
        "The plugin's identifier: agents submit to it, and it is unique among installed plugins.",
    ),
    (
        "version",
        true,
        "a semantic version such as `\"1.2.0\"`, or an integer",
        "The major version is a promise to every review created under it; a bare integer reads as `N.0.0`.",
    ),
    (
        "title",
        false,
        "a string; defaults to `name`",
        "What the app calls the plugin in its lists and settings.",
    ),
    (
        "icon",
        false,
        "a Lucide icon name",
        "Shown beside the plugin's reviews, such as `mail` or `git-pull-request`.",
    ),
    (
        "payload_schema",
        true,
        "a JSON Schema, or `{\"$ref\": \"file\"}`",
        "What an agent must send. Checked before a review reaches the inbox.",
    ),
    (
        "decision_schema",
        true,
        "a JSON Schema, or `{\"$ref\": \"file\"}`",
        "What the view hands back. Checked before the agent sees it.",
    ),
    (
        "entry",
        false,
        "a path in the folder; defaults to `index.html`",
        "The view's HTML file.",
    ),
    (
        "min_height",
        false,
        "a number of pixels; defaults to `400`",
        "The smallest height the app gives the view.",
    ),
    (
        "settings_schema",
        false,
        "a JSON Schema of scalars with defaults",
        "The plugin's own settings, each a row in Settings › Plugins. A bad one costs the plugin its settings, not its place.",
    ),
    (
        "shortcuts",
        false,
        "a list of `{keys, does, group?}`",
        "The keys the view answers: listed in the app's keyboard help and forwarded when the frame has no focus.",
    ),
    (
        "decision_template",
        false,
        "a path in the folder",
        "A MiniJinja template that renders a decision as markdown, for `--format markdown` and Copy as markdown.",
    ),
    (
        "build",
        false,
        "`{\"command\": \"…\"}`",
        "The command that produces the bundle, run by an install from a folder or a repository, never by a release.",
    ),
    (
        "dev",
        false,
        "`true` or `false`",
        "Marks a plugin under development in the app's listings.",
    ),
];

pub(super) const MANIFEST: &str = "manifest.json";

#[derive(Debug)]
pub struct Plugin {
    pub name: String,
    /// The major version: the line a review renders from.
    pub version: u32,
    /// The exact version, `1.2.3`; an integer in the manifest is `N.0.0`.
    pub release: String,
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
    /// The keys the view answers, as the manifest declares them: the app
    /// lists them and hands them to the view whether or not the frame has
    /// focus. `shortcuts_error` says why a declared list was dropped.
    pub shortcuts: Vec<Value>,
    pub shortcuts_error: Option<String>,
    /// The plugin's own markdown rendering of a decision (the manifest's
    /// `decision_template`, a MiniJinja file beside it), compiled at load;
    /// `template_error` says why a declared one was dropped.
    pub decision_template: Option<String>,
    pub template_error: Option<String>,
    /// How the plugin got here: a link served live, or a store entry with
    /// its record; none for the built-in and for a configured directory.
    pub install: Option<Install>,
    /// Set when the plugin could not be loaded; it is listed but unusable.
    pub error: Option<String>,
}

/// What the registry knows about an installed plugin, for its row.
#[derive(Debug, Clone)]
pub struct Install {
    pub kind: String,
    pub source: String,
    pub version: String,
    pub linked: bool,
    pub commit: Option<String>,
    /// for a release, the tag it came from
    pub tag: Option<String>,
    /// for a release, the SHA-256 of the asset downloaded
    pub asset_hash: Option<String>,
    pub hash: Option<String>,
    /// the store entry's files no longer match the hash recorded at install
    pub modified: bool,
    pub installed_at: String,
}

impl Install {
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "kind": self.kind,
            "source": self.source,
            "version": self.version,
            "linked": self.linked,
            "commit": self.commit,
            "tag": self.tag,
            "asset_hash": self.asset_hash,
            "hash": self.hash,
            "modified": self.modified,
            "installed_at": self.installed_at,
        })
    }
}

/// A manifest's version as text and its major: an integer is `N.0.0`,
/// a semantic version keeps its text. None for anything else.
pub fn version_of(value: &Value) -> Option<(String, i64)> {
    match value {
        Value::Number(n) => {
            let v = n.as_u64().filter(|v| *v > 0)?;
            Some((format!("{v}.0.0"), v as i64))
        }
        Value::String(s) => {
            let parts: Vec<&str> = s.trim().split('.').collect();
            if parts.len() != 3
                || parts
                    .iter()
                    .any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()))
            {
                return None;
            }
            let major: i64 = parts[0].parse().ok()?;
            Some((s.trim().to_string(), major))
        }
        _ => None,
    }
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
                release: "0.0.0".into(),
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
                shortcuts: Vec::new(),
                shortcuts_error: None,
                decision_template: None,
                template_error: None,
                install: None,
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
        // A major of 0 is a plugin still finding its shape, and fine; the one
        // release refused is the placeholder a broken plugin is listed under.
        let (release, major) = manifest
            .get("version")
            .and_then(version_of)
            .filter(|(release, _)| release != "0.0.0")
            .ok_or(
                "version is required: a positive integer, or a semantic version like \"1.2.0\"",
            )?;
        let version = major as u32;
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

        // likewise a bad shortcuts list
        let (shortcuts, shortcuts_error) = match manifest.get("shortcuts") {
            None | Some(Value::Null) => (Vec::new(), None),
            Some(raw) => match shortcuts::load(raw) {
                Ok(list) => (list, None),
                Err(message) => (Vec::new(), Some(message)),
            },
        };
        let (decision_template, template_error) = match manifest.get("decision_template") {
            None | Some(Value::Null) => (None, None),
            Some(Value::String(file))
                if !file.is_empty() && !file.contains("..") && !file.starts_with('/') =>
            {
                match std::fs::read_to_string(dir.join(file)) {
                    Ok(source) => match crate::markdown::compile(&source) {
                        Ok(()) => (Some(source), None),
                        Err(message) => (None, Some(format!("{file}: {message}"))),
                    },
                    Err(e) => (None, Some(format!("{file}: cannot read ({e})"))),
                }
            }
            Some(_) => (
                None,
                Some("decision_template must name a file beside the manifest".into()),
            ),
        };

        Ok(Plugin {
            title: manifest
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| name.clone()),
            name,
            version,
            release,
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
            shortcuts,
            shortcuts_error,
            decision_template,
            template_error,
            install: None,
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
            } else {
                property
                    .get("oneOf")
                    .and_then(Value::as_array)?
                    .iter()
                    .filter_map(|i| i.get("const"))
                    .map(Value::to_string)
                    .collect()
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
            "release": self.release,
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
            "shortcuts": self.shortcuts,
            "shortcuts_error": self.shortcuts_error,
            "template_error": self.template_error,
            "install": self.install.as_ref().map(Install::to_json),
        })
    }
}

/// The keys a plugin's view answers: a list of `{keys, does, group?}`,
/// `keys` in the app's shortcut form (`cmd+shift+m`, `j`, `shift+/`).
mod shortcuts {
    use serde_json::{Map, Value};

    const MODIFIERS: &[&str] = &[
        "cmd",
        "command",
        "super",
        "meta",
        "ctrl",
        "control",
        "alt",
        "option",
        "shift",
        "cmdorctrl",
        "commandorcontrol",
    ];

    pub fn load(raw: &Value) -> Result<Vec<Value>, String> {
        let Value::Array(items) = raw else {
            return Err("shortcuts must be a list of {keys, does}".into());
        };
        let mut out = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let Value::Object(entry) = item else {
                return Err(format!(
                    "shortcuts[{i}] must be an object with keys and does"
                ));
            };
            let keys = match entry.get("keys").and_then(Value::as_str) {
                Some(k) if !k.trim().is_empty() => normalize(k).ok_or_else(|| {
                    format!("shortcuts[{i}]: keys {k:?} is not a key combination")
                })?,
                _ => return Err(format!("shortcuts[{i}] needs keys, a string")),
            };
            let does = match entry.get("does").and_then(Value::as_str) {
                Some(d) if !d.trim().is_empty() => d.trim().to_string(),
                _ => return Err(format!("shortcuts[{i}] needs does, a string")),
            };
            let mut clean = Map::new();
            clean.insert("keys".into(), Value::String(keys));
            clean.insert("does".into(), Value::String(does));
            match entry.get("group") {
                None | Some(Value::Null) => {}
                Some(Value::String(g)) if !g.trim().is_empty() => {
                    clean.insert("group".into(), Value::String(g.trim().to_string()));
                }
                Some(_) => return Err(format!("shortcuts[{i}]: group must be a string")),
            }
            out.push(Value::Object(clean));
        }
        Ok(out)
    }

    /// Modifiers in any order and case, then one key; back in lowercase.
    fn normalize(keys: &str) -> Option<String> {
        let parts: Vec<String> = keys.split('+').map(|p| p.trim().to_lowercase()).collect();
        let (key, modifiers) = parts.split_last()?;
        if key.is_empty() || key.chars().any(char::is_whitespace) {
            return None;
        }
        if modifiers.iter().any(|m| !MODIFIERS.contains(&m.as_str())) {
            return None;
        }
        Some(parts.join("+"))
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

pub(super) fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
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

#[cfg(test)]
mod tests {
    /// A key a plugin in this repository uses is a key the docs describe.
    #[test]
    fn every_key_the_sample_plugins_use_is_documented() {
        let plugins = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        for entry in std::fs::read_dir(&plugins).unwrap().flatten() {
            let Ok(text) = std::fs::read_to_string(entry.path().join("manifest.json")) else {
                continue;
            };
            let manifest: serde_json::Map<String, serde_json::Value> =
                serde_json::from_str(&text).unwrap();
            for key in manifest.keys() {
                assert!(
                    super::KEYS.iter().any(|(k, ..)| k == key),
                    "{key}, in {}, is not in KEYS",
                    entry.path().display()
                );
            }
        }
    }

    use super::*;

    #[test]
    fn a_plugin_name_is_lowercase_words_joined_by_underscores_or_dashes() {
        for ok in ["list", "code_review", "code-review", "a1", "x"] {
            assert!(super::valid_name(ok), "{ok}");
        }
        // it names a folder, a URL path and a CLI argument: keep it plain
        for bad in [
            "",
            "List",
            "1list",
            "_list",
            "-list",
            "code review",
            "code.review",
            "café",
        ] {
            assert!(!super::valid_name(bad), "{bad}");
        }
    }

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

    #[test]
    fn a_version_reads_as_its_release_and_major_and_zero_point_x_is_a_plugin_too() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = plugin_dir(tmp.path(), "young", "");
        let manifest = dir.join("manifest.json");
        let with = |version: &str| {
            let text = std::fs::read_to_string(&manifest).unwrap();
            let re = regex_lite_version(&text, version);
            std::fs::write(&manifest, re).unwrap();
            Plugin::load(&dir)
        };
        let p = with("\"0.1.0\"");
        assert_eq!(p.error, None, "{:?}", p.error);
        assert_eq!((p.version, p.release.as_str()), (0, "0.1.0"));
        let p = with("\"2.3.4\"");
        assert_eq!((p.version, p.release.as_str()), (2, "2.3.4"));
        let p = with("3");
        assert_eq!((p.version, p.release.as_str()), (3, "3.0.0"));
        for bad in ["\"0.0.0\"", "0", "\"1.2\"", "\"v1.2.0\"", "true"] {
            let p = with(bad);
            assert!(
                p.error
                    .as_deref()
                    .is_some_and(|e| e.starts_with("version is required")),
                "{bad}: {:?}",
                p.error
            );
        }
    }

    /// The manifest text with its `"version":…` field replaced.
    fn regex_lite_version(text: &str, version: &str) -> String {
        let start = text.find("\"version\":").unwrap() + "\"version\":".len();
        let end = start + text[start..].find(',').unwrap();
        format!("{}{version}{}", &text[..start], &text[end..])
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
    fn shortcuts_are_checked_and_normalized() {
        let tmp = tempfile::tempdir().unwrap();
        let good = r#","shortcuts":[{"keys":"j","does":"Next"},{"keys":" Cmd + Shift+M ","does":"Maximize","group":"View"},{"keys":"shift+/","does":"Help"}]"#;
        let p = Plugin::load(&plugin_dir(tmp.path(), "keys", good));
        assert!(p.usable(), "{:?}", p.error);
        assert_eq!(p.shortcuts_error, None);
        assert_eq!(
            p.shortcuts,
            vec![
                serde_json::json!({"keys": "j", "does": "Next"}),
                serde_json::json!({"keys": "cmd+shift+m", "does": "Maximize", "group": "View"}),
                serde_json::json!({"keys": "shift+/", "does": "Help"}),
            ]
        );
        assert_eq!(p.to_json()["shortcuts"][1]["keys"], "cmd+shift+m");

        let cases = [
            (r#","shortcuts":{"keys":"j"}"#, "must be a list"),
            (r#","shortcuts":[{"does":"Next"}]"#, "needs keys"),
            (r#","shortcuts":[{"keys":"j"}]"#, "needs does"),
            (
                r#","shortcuts":[{"keys":"hyper+j","does":"x"}]"#,
                "not a key combination",
            ),
            (
                r#","shortcuts":[{"keys":"j","does":"x","group":3}]"#,
                "group must be a string",
            ),
        ];
        for (i, (extra, expected)) in cases.iter().enumerate() {
            let p = Plugin::load(&plugin_dir(tmp.path(), &format!("k{i}"), extra));
            assert!(p.usable(), "{extra}: {:?}", p.error);
            assert!(p.shortcuts.is_empty());
            let why = p.shortcuts_error.clone().unwrap_or_default();
            assert!(why.contains(expected), "{extra}: {why}");
        }
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
}
