//! Loading plugin manifests and validating their schemas, settings and shortcuts.
//! Invalid definitions remain inspectable as plugins carrying an error.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::error::Violation;
use crate::schema::Schema;

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
    /// The plugin's icon, the SVG markup of the file its manifest names,
    /// shown wherever the plugin is named; `icon_error` says why a declared
    /// one was dropped.
    pub icon: Option<String>,
    pub icon_error: Option<String>,
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
    /// When an agent should ask with this plugin (the manifest's `use_when`).
    pub use_when: Option<String>,
    /// A payload that passes the payload schema (the manifest's `example`,
    /// a file beside it), for an agent to start from; `example_error` says
    /// why a declared one was dropped.
    pub example: Option<Value>,
    pub example_error: Option<String>,
    /// A review anyone can send to see the plugin (the manifest's
    /// `sample`, a request file beside it); `sample_error` says why a
    /// declared one was dropped.
    pub sample: Option<super::sample::Sample>,
    pub sample_error: Option<String>,
    /// The files the plugin takes beside a payload (the manifest's
    /// `attachments`); none takes none. A malformed block makes the plugin
    /// unusable, as a broken schema does.
    pub attachments: Option<crate::attachments::AttachmentRules>,
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

/// A manifest's semantic version as text and its major. None for anything
/// else, a bare number included.
pub fn version_of(value: &Value) -> Option<(String, i64)> {
    let text = value.as_str()?.trim();
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let major: i64 = parts[0].parse().ok()?;
    Some((text.to_string(), major))
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
                icon_error: None,
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
                use_when: None,
                example: None,
                example_error: None,
                sample: None,
                sample_error: None,
                attachments: None,
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
        // the manifest schema first: a violation outside the optional
        // features refuses the plugin, one inside them costs that feature
        let shape = shape::check(&manifest);
        if let Some(problem) = shape.problems.into_iter().next() {
            return Err(problem);
        }
        let Value::Object(manifest) = manifest else {
            return Err(format!("{MANIFEST} must be a JSON object"));
        };

        // the schema has checked the shapes; what is left is what it cannot
        // say, such as whether the files named are there
        let name = manifest["name"].as_str().unwrap_or_default().to_string();
        let (release, major) = version_of(&manifest["version"])
            .ok_or("version is not a semantic version like \"1.2.0\"")?;
        let version = major as u32;
        let entry = manifest
            .get("entry")
            .and_then(Value::as_str)
            .unwrap_or("index.html")
            .to_string();
        if !dir.join(&entry).is_file() {
            return Err(format!("entry {entry} not found"));
        }
        // an icon that does not load costs the plugin its icon, not its place
        let (icon, icon_error) = match manifest.get("icon") {
            _ if shape.dropped.contains_key("icon") => (None, shape.dropped.get("icon").cloned()),
            Some(Value::String(file)) => match icon_markup(dir, file) {
                Ok(svg) => (Some(svg), None),
                Err(message) => (None, Some(message)),
            },
            _ => (None, None),
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
                _ if shape.dropped.contains_key("settings_schema") => {
                    (None, None, shape.dropped.get("settings_schema").cloned())
                }
                None | Some(Value::Null) => (None, None, None),
                Some(raw) => match settings::load(dir, &name, version, raw) {
                    Ok((document, validator)) => (Some(document), Some(validator), None),
                    Err(message) => (None, None, Some(message)),
                },
            };

        // likewise a bad shortcuts list
        let (shortcuts, shortcuts_error) = match manifest.get("shortcuts") {
            _ if shape.dropped.contains_key("shortcuts") => {
                (Vec::new(), shape.dropped.get("shortcuts").cloned())
            }
            None | Some(Value::Null) => (Vec::new(), None),
            Some(raw) => match shortcuts::load(raw) {
                Ok(list) => (list, None),
                Err(message) => (Vec::new(), Some(message)),
            },
        };
        let (decision_template, template_error) = match manifest.get("decision_template") {
            _ if shape.dropped.contains_key("decision_template") => {
                (None, shape.dropped.get("decision_template").cloned())
            }
            None | Some(Value::Null) => (None, None),
            Some(Value::String(file)) => match std::fs::read_to_string(dir.join(file)) {
                Ok(source) => match crate::markdown::compile(&source) {
                    Ok(()) => (Some(source), None),
                    Err(message) => (None, Some(format!("{file}: {message}"))),
                },
                Err(e) => (None, Some(format!("{file}: cannot read ({e})"))),
            },
            Some(_) => (None, None),
        };
        // an example that does not pass the plugin's own schema is dropped,
        // so what an agent is shown always submits
        let (example, example_error) = match manifest.get("example") {
            _ if shape.dropped.contains_key("example") => {
                (None, shape.dropped.get("example").cloned())
            }
            None | Some(Value::Null) => (None, None),
            Some(Value::String(file)) => match std::fs::read_to_string(dir.join(file))
                .map_err(|e| format!("{file}: cannot read ({e})"))
                .and_then(|text| {
                    serde_json::from_str::<Value>(&text)
                        .map_err(|e| format!("{file}: not JSON ({e})"))
                }) {
                Ok(payload) => match payload_schema.validate(&payload).first() {
                    None => (Some(payload), None),
                    Some(v) => (
                        None,
                        Some(format!(
                            "{file}: does not pass payload_schema at {}: {}",
                            if v.path.is_empty() { "/" } else { &v.path },
                            v.message
                        )),
                    ),
                },
                Err(message) => (None, Some(message)),
            },
            Some(_) => (None, None),
        };
        // likewise a sample that does not load
        let (sample, sample_error) = match manifest.get("sample") {
            _ if shape.dropped.contains_key("sample") => {
                (None, shape.dropped.get("sample").cloned())
            }
            Some(Value::String(file)) => match super::sample::load(dir, file, &payload_schema) {
                Ok(sample) => (Some(sample), None),
                Err(message) => (None, Some(message)),
            },
            _ => (None, None),
        };
        let use_when = manifest
            .get("use_when")
            .and_then(Value::as_str)
            .map(str::to_string);
        let attachments = match manifest.get("attachments") {
            None | Some(Value::Null) => None,
            Some(block) => Some(crate::attachments::AttachmentRules::parse(block)?),
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
            icon_error,
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
            use_when,
            example,
            example_error,
            sample,
            sample_error,
            attachments,
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

    /// What an agent needs to ask with the plugin: what it is for, when to
    /// use it, the schemas with a top-level `$ref` read in, and an example.
    pub fn describe(&self) -> Value {
        serde_json::json!({
            "name": self.name,
            "title": self.title,
            "version": self.version,
            "release": self.release,
            "description": self.manifest.get("description"),
            "use_when": self.use_when,
            "payload_schema": self.schema_document("payload_schema"),
            "decision_schema": self.schema_document("decision_schema"),
            "example": self.example,
            "sample": self.sample.is_some(),
            "attachments": self.manifest.get("attachments"),
            "markdown": self.decision_template.is_some(),
        })
    }

    /// The manifest's schema under `key`, or the file its `$ref` names when
    /// that is all it holds.
    fn schema_document(&self, key: &str) -> Value {
        let raw = self.manifest.get(key).cloned().unwrap_or(Value::Null);
        let Some(reference) = raw
            .as_object()
            .filter(|map| map.len() == 1)
            .and_then(|map| map.get("$ref"))
            .and_then(Value::as_str)
        else {
            return raw;
        };
        crate::schema::safe_join(&self.path, reference)
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(raw)
    }

    /// What the app makes of the folder, as `pinrail-plugin check --json`
    /// says it: usable or not, why it would be refused, and each feature it
    /// would drop, keyed by the manifest key.
    pub fn verdict(&self) -> Value {
        let warnings: Vec<Value> = [
            ("settings_schema", &self.settings_error),
            ("shortcuts", &self.shortcuts_error),
            ("decision_template", &self.template_error),
            ("example", &self.example_error),
            ("sample", &self.sample_error),
            ("icon", &self.icon_error),
        ]
        .into_iter()
        .filter_map(|(key, error)| {
            error
                .as_ref()
                .map(|message| serde_json::json!({ "key": key, "message": message }))
        })
        .collect();
        serde_json::json!({
            "usable": self.usable(),
            "name": self.usable().then_some(&self.name),
            "release": self.usable().then_some(&self.release),
            "problems": self.error.iter().map(|message| serde_json::json!({ "message": message })).collect::<Vec<_>>(),
            "warnings": if self.usable() { warnings } else { Vec::new() },
        })
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
            "icon_error": self.icon_error,
            "usable": self.usable(),
            "error": self.error,
            "payload_schema": self.manifest.get("payload_schema"),
            "decision_schema": self.manifest.get("decision_schema"),
            "settings_schema": self.settings_schema,
            "settings_error": self.settings_error,
            "shortcuts": self.shortcuts,
            "shortcuts_error": self.shortcuts_error,
            "template_error": self.template_error,
            "description": self.manifest.get("description"),
            "use_when": self.use_when,
            "example_error": self.example_error,
            "sample": self.sample.is_some(),
            "sample_error": self.sample_error,
            "attachments": self.manifest.get("attachments"),
            "install": self.install.as_ref().map(Install::to_json),
        })
    }
}

/// The largest icon file the app takes: an icon is a few paths.
const ICON_MAX_BYTES: u64 = 32 * 1024;

/// The markup of a plugin's icon, the SVG file `file` names inside `dir`.
/// The app draws it as a mask, in the text's colour, so its shapes count and
/// its colours do not; nothing in it runs.
pub fn icon_markup(dir: &Path, file: &str) -> Result<String, String> {
    let path = crate::schema::safe_join(dir, file)
        .ok_or_else(|| format!("{file}: outside the plugin's folder"))?;
    let size = std::fs::metadata(&path)
        .map_err(|e| format!("{file}: cannot read ({e})"))?
        .len();
    if size > ICON_MAX_BYTES {
        return Err(format!(
            "{file}: {size} bytes; an icon is at most {ICON_MAX_BYTES}"
        ));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{file}: cannot read ({e})"))?;
    let svg = text.trim();
    // an XML declaration or a comment may come first; the root is <svg>
    let start = svg
        .find("<svg")
        .ok_or_else(|| format!("{file}: not an SVG"))?;
    if !svg.ends_with("</svg>") && !svg.ends_with("/>") {
        return Err(format!("{file}: not an SVG"));
    }
    Ok(svg[start..].to_string())
}

/// The manifest held to its JSON Schema, `manifest.schema.json` in the
/// pinrail-plugin package, which build.rs copies in: the one description of a
/// manifest, shared with authors' editors and the docs.
#[cfg(feature = "docs")]
pub use shape::SCHEMA;

mod shape {
    use std::collections::BTreeMap;
    use std::sync::OnceLock;

    use serde_json::Value;

    use crate::schema::Schema;

    /// The schema's text, as the pinrail-plugin package ships it.
    pub const SCHEMA: &str = include_str!(concat!(env!("OUT_DIR"), "/manifest.schema.json"));

    /// Keys whose violation costs the plugin that feature, not its place.
    const FEATURES: &[&str] = &[
        "settings_schema",
        "shortcuts",
        "decision_template",
        "example",
        "sample",
        "icon",
    ];

    pub struct Shape {
        /// violations that refuse the plugin, as `path: message`
        pub problems: Vec<String>,
        /// the first violation under each feature key, by key
        pub dropped: BTreeMap<&'static str, String>,
    }

    fn schema() -> &'static Schema {
        static COMPILED: OnceLock<Schema> = OnceLock::new();
        COMPILED.get_or_init(|| {
            let document: Value =
                serde_json::from_str(SCHEMA).expect("manifest.schema.json is JSON");
            Schema::standalone(&document).expect("manifest.schema.json compiles")
        })
    }

    pub fn check(manifest: &Value) -> Shape {
        let mut shape = Shape {
            problems: Vec::new(),
            dropped: BTreeMap::new(),
        };
        for violation in schema().validate(manifest) {
            let text = match violation.path.trim_start_matches('/') {
                "" => violation.message,
                path => format!("{path}: {}", violation.message),
            };
            let key = violation
                .path
                .trim_start_matches('/')
                .split('/')
                .next()
                .unwrap_or("");
            match FEATURES.iter().find(|f| **f == key) {
                Some(feature) => {
                    shape.dropped.entry(feature).or_insert(text);
                }
                None => shape.problems.push(text),
            }
        }
        shape
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

#[cfg(test)]
mod tests {

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

    /// A plugin directory with the given manifest fields on top of the
    /// minimum, and an empty view.
    fn plugin_dir(root: &Path, name: &str, extra: &str) -> PathBuf {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "<html></html>").unwrap();
        std::fs::write(
            dir.join("manifest.json"),
            format!(
                "{{\"name\":\"{name}\",\"version\":\"1.0.0\",\"payload_schema\":{{}},\"decision_schema\":{{}}{extra}}}"
            ),
        )
        .unwrap();
        dir
    }

    /// A plugin folder with exactly this manifest and an index.html.
    fn with_manifest(root: &Path, folder: &str, manifest: serde_json::Value) -> PathBuf {
        let dir = root.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "<html></html>").unwrap();
        std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
        dir
    }

    /// The smallest manifest the schema accepts, with `changes` laid over
    /// it; a null in `changes` removes the key.
    fn manifest(changes: serde_json::Value) -> serde_json::Value {
        let mut m = serde_json::json!({"name": "sample", "version": "1.0.0", "payload_schema": {}, "decision_schema": {}});
        for (k, v) in changes.as_object().unwrap() {
            if v.is_null() {
                m.as_object_mut().unwrap().remove(k);
            } else {
                m[k] = v.clone();
            }
        }
        m
    }

    #[test]
    fn an_icon_is_the_markup_of_its_svg_file_and_a_bad_one_costs_only_the_icon() {
        use serde_json::json;
        let tmp = tempfile::tempdir().unwrap();
        let ok = with_manifest(
            tmp.path(),
            "ok",
            manifest(json!({"icon": "icons/mark.svg"})),
        );
        std::fs::create_dir_all(ok.join("icons")).unwrap();
        std::fs::write(
            ok.join("icons/mark.svg"),
            "<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><path d=\"M4 4h16\"/></svg>\n",
        )
        .unwrap();
        let p = Plugin::load(&ok);
        assert!(
            p.icon
                .as_deref()
                .unwrap_or_default()
                .starts_with("<svg xmlns"),
            "{:?}",
            p.icon
        );
        assert!(p.icon_error.is_none());

        for (i, (changes, why)) in [
            (json!({"icon": "mail"}), "icon: "),
            (json!({"icon": "missing.svg"}), "missing.svg: cannot read"),
            (json!({"icon": "../outside.svg"}), "icon: "),
            (json!({"icon": "not.svg"}), "not.svg: not an SVG"),
        ]
        .into_iter()
        .enumerate()
        {
            let dir = with_manifest(tmp.path(), &format!("bad{i}"), manifest(changes.clone()));
            std::fs::write(dir.join("not.svg"), "hello").unwrap();
            let p = Plugin::load(&dir);
            assert!(p.usable(), "{changes}: {:?}", p.error);
            assert!(p.icon.is_none(), "{changes}");
            let error = p.icon_error.clone().unwrap_or_default();
            assert!(error.starts_with(why), "{changes}: {error}");
        }
    }

    #[test]
    fn a_manifest_that_breaks_its_schema_is_refused_and_says_where() {
        use serde_json::json;
        let tmp = tempfile::tempdir().unwrap();
        let cases = [
            (json!({"name": null}), "property 'name' is required"),
            (json!({"version": null}), "property 'version' is required"),
            (
                json!({"payload_schema": null}),
                "property 'payload_schema' is required",
            ),
            (
                json!({"decision_schema": null}),
                "property 'decision_schema' is required",
            ),
            (json!({"name": "Sample"}), "name: "),
            (json!({"name": "1sample"}), "name: "),
            (json!({"name": "sam ple"}), "name: "),
            (json!({"name": 7}), "name: "),
            (json!({"version": "0.0.0"}), "version: "),
            (json!({"version": 0}), "version: "),
            (json!({"version": -1}), "version: "),
            (json!({"version": "1.2"}), "version: "),
            (json!({"version": "v1.2.0"}), "version: "),
            (json!({"version": true}), "version: "),
            (json!({"title": 3}), "title: "),
            (json!({"description": ["a"]}), "description: "),
            (
                json!({"payload_schema": "schemas/payload.json"}),
                "payload_schema: ",
            ),
            (json!({"decision_schema": []}), "decision_schema: "),
            (json!({"entry": ""}), "entry: "),
            (json!({"entry": "/etc/index.html"}), "entry: "),
            (json!({"entry": 3}), "entry: "),
            (json!({"min_height": 0}), "min_height: "),
            (json!({"min_height": "400"}), "min_height: "),
            (json!({"min_height": 12.5}), "min_height: "),
            (json!({"dev": "yes"}), "dev: "),
            (json!({"build": "npm run build"}), "build: "),
            (
                json!({"build": {}}),
                "build: property 'command' is required",
            ),
            (json!({"build": {"command": "   "}}), "build/command: "),
        ];
        for (i, (changes, expected)) in cases.iter().enumerate() {
            let dir = with_manifest(tmp.path(), &format!("bad{i}"), manifest(changes.clone()));
            let p = Plugin::load(&dir);
            assert!(!p.usable(), "{changes} loaded");
            let error = p.error.clone().unwrap_or_default();
            assert!(error.contains(expected), "{changes}: {error}");
        }
        // not JSON, or not an object, before the schema is asked
        for (i, text) in ["{name", "[]", "\"sample\""].iter().enumerate() {
            let dir = tmp.path().join(format!("text{i}"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("manifest.json"), text).unwrap();
            assert!(!Plugin::load(&dir).usable(), "{text} loaded");
        }
        // and a file the schema cannot see: the entry must be there
        let dir = with_manifest(
            tmp.path(),
            "no-entry",
            manifest(serde_json::json!({"entry": "view/index.html"})),
        );
        assert_eq!(
            Plugin::load(&dir).error.as_deref(),
            Some("entry view/index.html not found")
        );
    }

    #[test]
    fn a_broken_optional_feature_costs_only_that_feature() {
        use serde_json::json;
        let tmp = tempfile::tempdir().unwrap();
        let cases = [
            (
                json!({"settings_schema": "settings.json"}),
                "settings_schema",
                "settings_schema: ",
            ),
            (
                json!({"settings_schema": 3}),
                "settings_schema",
                "settings_schema: ",
            ),
            (json!({"shortcuts": "j"}), "shortcuts", "shortcuts: "),
            (
                json!({"shortcuts": [{"keys": "", "does": "Next"}]}),
                "shortcuts",
                "shortcuts/0/keys: ",
            ),
            (
                json!({"shortcuts": [{"keys": "j", "does": ""}]}),
                "shortcuts",
                "shortcuts/0/does: ",
            ),
            (
                json!({"decision_template": "../outside.j2"}),
                "decision_template",
                "decision_template: ",
            ),
            (
                json!({"decision_template": "/etc/decision.j2"}),
                "decision_template",
                "decision_template: ",
            ),
            (
                json!({"decision_template": ""}),
                "decision_template",
                "decision_template: ",
            ),
            (
                json!({"decision_template": 1}),
                "decision_template",
                "decision_template: ",
            ),
        ];
        for (i, (changes, feature, expected)) in cases.iter().enumerate() {
            let dir = with_manifest(
                tmp.path(),
                &format!("feature{i}"),
                manifest(changes.clone()),
            );
            let p = Plugin::load(&dir);
            assert!(p.usable(), "{changes}: {:?}", p.error);
            let why = match *feature {
                "settings_schema" => p.settings_error.clone(),
                "shortcuts" => p.shortcuts_error.clone(),
                _ => p.template_error.clone(),
            }
            .unwrap_or_default();
            assert!(why.starts_with(expected), "{changes}: {why}");
        }
    }

    #[test]
    fn an_example_is_kept_when_it_passes_the_payload_schema_and_dropped_when_not() {
        use serde_json::json;
        let tmp = tempfile::tempdir().unwrap();
        let with = |folder: &str, example: &str| {
            let dir = with_manifest(
                tmp.path(),
                folder,
                manifest(
                    json!({"payload_schema": {"type": "object", "required": ["n"]}, "example": "example.json", "use_when": "Before posting"}),
                ),
            );
            std::fs::write(dir.join("example.json"), example).unwrap();
            Plugin::load(&dir)
        };
        let good = with("good", r#"{"n": 1}"#);
        assert_eq!(good.example, Some(json!({"n": 1})));
        assert_eq!(good.use_when.as_deref(), Some("Before posting"));
        let bad = with("bad", r#"{"m": 1}"#);
        assert!(bad.usable() && bad.example.is_none());
        assert!(
            bad.example_error
                .as_deref()
                .unwrap()
                .contains("does not pass payload_schema"),
            "{:?}",
            bad.example_error
        );
        let broken = with("broken", "{");
        assert!(
            broken
                .example_error
                .as_deref()
                .unwrap()
                .contains("not JSON")
        );
        let missing = Plugin::load(&with_manifest(
            tmp.path(),
            "missing",
            manifest(json!({"example": "nope.json"})),
        ));
        assert!(
            missing.usable()
                && missing
                    .example_error
                    .as_deref()
                    .unwrap()
                    .contains("cannot read")
        );
    }

    #[test]
    fn what_the_schema_allows_loads() {
        use serde_json::json;
        let tmp = tempfile::tempdir().unwrap();
        let cases = [
            json!({}),
            json!({"version": "3.0.0"}),
            json!({"version": "0.1.0"}),
            json!({"$schema": "https://pinrail.dev/schemas/manifest.schema.json"}),
            json!({"a_key_from_a_newer_app": {"anything": true}}),
            json!({"icon": serde_json::Value::Null, "settings_schema": serde_json::Value::Null}),
            json!({"title": "Sample", "description": "A sample.", "icon": "git-pull-request", "min_height": 200, "dev": true}),
            json!({"build": {"command": "npm ci && npm run build"}}),
            json!({"shortcuts": [{"keys": "cmd+shift+f", "does": "Fold", "group": "View"}]}),
        ];
        for (i, changes) in cases.iter().enumerate() {
            let dir = with_manifest(tmp.path(), &format!("good{i}"), manifest(changes.clone()));
            let p = Plugin::load(&dir);
            assert!(p.usable(), "{changes}: {:?}", p.error);
            assert_eq!(
                (
                    p.settings_error.clone(),
                    p.shortcuts_error.clone(),
                    p.template_error.clone()
                ),
                (None, None, None),
                "{changes}"
            );
        }
    }

    #[test]
    fn every_sample_plugin_meets_the_manifest_schema() {
        let plugins = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        for entry in std::fs::read_dir(&plugins).unwrap().flatten() {
            let Ok(text) = std::fs::read_to_string(entry.path().join("manifest.json")) else {
                continue;
            };
            let shape = super::shape::check(&serde_json::from_str(&text).unwrap());
            assert!(
                shape.problems.is_empty() && shape.dropped.is_empty(),
                "{}: {:?} {:?}",
                entry.path().display(),
                shape.problems,
                shape.dropped
            );
        }
    }

    #[test]
    fn a_semantic_version_reads_as_its_release_and_major_and_zero_point_x_is_a_plugin_too() {
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
        // a bare number is not a semantic version
        for bad in ["\"0.0.0\"", "0", "3", "\"1.2\"", "\"v1.2.0\"", "true"] {
            let p = with(bad);
            assert!(
                p.error
                    .as_deref()
                    .is_some_and(|e| e.starts_with("version: ")),
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
            (r#","shortcuts":{"keys":"j"}"#, "not of type null or array"),
            (
                r#","shortcuts":[{"does":"Next"}]"#,
                "property 'keys' is required",
            ),
            (
                r#","shortcuts":[{"keys":"j"}]"#,
                "property 'does' is required",
            ),
            (
                r#","shortcuts":[{"keys":"hyper+j","does":"x"}]"#,
                "not a key combination",
            ),
            (
                r#","shortcuts":[{"keys":"j","does":"x","group":3}]"#,
                "shortcuts/0/group:",
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
