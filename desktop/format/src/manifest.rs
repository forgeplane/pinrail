//! Loading plugin manifests and validating their schemas, settings and shortcuts.
//! Invalid definitions remain inspectable as plugins carrying an error.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::Violation;
use crate::schema::Schema;

pub const MANIFEST: &str = "manifest.json";

#[derive(Debug)]
pub struct Plugin {
    pub name: String,
    /// The semantic version, such as `1.2.3`.
    pub version: String,
    pub title: String,
    pub path: PathBuf,
    pub min_height: u32,
    pub dev: bool,
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
    /// How the plugin's reviews are summed up (the manifest's `summary`):
    /// what a request asks and what was decided; `summary_error` says why a
    /// declared one was dropped. Empty when none is declared.
    pub summary: crate::summary::Declaration,
    pub summary_error: Option<String>,
    /// When an agent should ask with this plugin (the manifest's `use_when`).
    pub use_when: Option<String>,
    /// Whole reviews anyone can send to see the plugin, from
    /// `samples/<name>.json`, in order of name. The first one's payload is
    /// the example an agent is shown. `sample_errors` says why each one
    /// that does not load was dropped.
    pub samples: Vec<crate::sample::Sample>,
    pub sample_errors: Vec<String>,
    /// Top-level manifest keys the schema does not define: a typo, or a key
    /// a newer Pinrail reads. Kept, and warned about by a check.
    pub unknown_keys: Vec<String>,
    /// The files the plugin takes beside a payload (the manifest's
    /// `attachments`); none takes none. A malformed block makes the plugin
    /// unusable, as a broken schema does.
    pub attachments: Option<crate::attachments::AttachmentRules>,
    /// How the plugin got here: its full name, its source and its bundles.
    /// None for a plugin read from a folder by itself, as a
    /// check reads one.
    pub install: Option<Install>,
    /// Set when the plugin could not be loaded; it is listed but unusable.
    pub error: Option<String>,
}

/// What the registry knows about an installed plugin, for its row.
#[derive(Debug, Clone)]
pub struct Install {
    /// The full name, `<publisher>/<name>`.
    pub plugin: String,
    pub publisher: String,
    /// `bundled`, `folder`, `link`, `git` or `release`
    pub kind: String,
    pub source: String,
    pub linked: bool,
    pub commit: Option<String>,
    /// for a release, the tag it came from
    pub tag: Option<String>,
    /// for a release, the SHA-256 of the asset downloaded
    pub asset_hash: Option<String>,
    /// The bundle new reviews render with; none for a link.
    pub bundle: Option<String>,
    /// the bundle's files no longer match its listing
    pub modified: bool,
    pub installed_at: String,
    pub updated_at: String,
    /// The release the current one replaced, while it can be rolled back to.
    pub previous: Option<Previous>,
}

/// A release an installation can be rolled back to, until when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Previous {
    pub version: String,
    pub bundle: String,
    pub until: String,
}

impl Install {
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "kind": self.kind,
            "source": self.source,
            "linked": self.linked,
            "commit": self.commit,
            "tag": self.tag,
            "asset_hash": self.asset_hash,
            "bundle": self.bundle,
            "previous": self.previous.as_ref().map(|p| serde_json::json!({
                "version": p.version, "bundle": p.bundle, "until": p.until,
            })),
            "modified": self.modified,
            "installed_at": self.installed_at,
            "updated_at": self.updated_at,
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

/// The part of a version within which semantic versioning promises
/// compatibility: the major from 1 on, and below it the major and minor,
/// as Cargo reads `0.x` versions. `1.4.2` gives `1`, `0.3.1` gives `0.3`
/// and `0.0.4` gives `0.0`. A version that is not three numbers has none.
pub fn line_of(version: &str) -> Option<String> {
    let (version, _) = version_of(&Value::from(version))?;
    let mut parts = version.split('.').map(|p| p.parse::<u64>());
    let (Some(Ok(major)), Some(Ok(minor))) = (parts.next(), parts.next()) else {
        return None;
    };
    Some(if major > 0 {
        major.to_string()
    } else {
        format!("0.{minor}")
    })
}

/// What a check says of a manifest key the schema does not define.
const UNKNOWN_KEY: &str =
    "not a manifest key: a typo, or a key for a newer Pinrail; the app ignores it";

/// The view's page: the frame loads it, and everything it loads is beside
/// it under `view/`.
pub const VIEW: &str = "view/index.html";
/// The plugin's icon, when it has one.
pub const ICON: &str = "icon.svg";
/// The payload's schema.
pub const PAYLOAD_SCHEMA: &str = "schemas/payload.schema.json";
/// The decision's schema.
pub const DECISION_SCHEMA: &str = "schemas/decision.schema.json";
/// The decision as markdown, when the plugin renders its own.
pub const TEMPLATE: &str = "templates/decision.md.j2";

/// Keys that once named a file, and where that file always is now: a
/// manifest that still has one is told so rather than that the key is
/// unknown.
const PLACED: &[(&str, &str)] = &[
    ("entry", "the view is always view/index.html"),
    ("icon", "the icon is always icon.svg"),
    (
        "payload_schema",
        "the payload schema is always schemas/payload.schema.json",
    ),
    (
        "decision_schema",
        "the decision schema is always schemas/decision.schema.json",
    ),
    (
        "decision_template",
        "the template is always templates/decision.md.j2",
    ),
    (
        "example",
        "the example is the first sample's payload, in samples/",
    ),
    ("sample", "samples are samples/<name>.json"),
];

impl Plugin {
    /// Loads the plugin at `dir`. Never fails: a bad plugin comes back with `error`.
    pub fn load(dir: &Path) -> Plugin {
        match Self::try_load(dir, false) {
            Ok(plugin) => plugin,
            Err(message) => Plugin {
                name: dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                version: "0.0.0".into(),
                title: String::new(),
                path: dir.to_path_buf(),
                min_height: 400,
                dev: false,
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
                summary: Default::default(),
                summary_error: None,
                use_when: None,
                samples: Vec::new(),
                sample_errors: Vec::new(),
                unknown_keys: Vec::new(),
                attachments: None,
                install: None,
                error: Some(message),
            },
        }
    }

    /// What `plugins check` answers for a folder: the loader's verdict,
    /// except that a plugin whose build writes its entry is judged before
    /// that build, as an install takes it, with the missing entry a warning.
    pub fn check(dir: &Path) -> Value {
        let Ok(plugin) = Self::try_load(dir, true) else {
            return Self::load(dir).verdict();
        };
        let mut verdict = plugin.verdict();
        // advice that costs the plugin nothing
        let mut notes = Vec::new();
        if plugin.samples.is_empty() && plugin.sample_errors.is_empty() {
            notes.push(serde_json::json!({
                "key": "samples",
                "message": "no samples: agents get no example payload to start from, and people no review to try; add samples/<name>.json",
            }));
        }
        verdict["notes"] = Value::Array(notes);
        let built = dir.join(VIEW).is_file();
        // the bundle an install would make of the folder, once its view is
        // there; one it could not make refuses the folder
        if built {
            match crate::bundle::Listing::of_folder(dir, crate::bundle::Taken::FromSource) {
                Ok(listing) => {
                    verdict["bundle"] = serde_json::json!({
                        "hash": listing.hash(),
                        "files": listing.files.len(),
                        "size": listing.size(),
                    });
                }
                Err(message) => {
                    verdict["usable"] = Value::Bool(false);
                    verdict["name"] = Value::Null;
                    verdict["version"] = Value::Null;
                    verdict["warnings"] = serde_json::json!([]);
                    if let Some(problems) = verdict["problems"].as_array_mut() {
                        problems.push(serde_json::json!({ "message": message }));
                    }
                }
            }
        }
        if !built {
            let command = plugin
                .manifest
                .get("build")
                .and_then(|build| build["command"].as_str())
                .unwrap_or_default()
                .trim();
            if let Some(warnings) = verdict["warnings"].as_array_mut() {
                warnings.insert(
                    0,
                    serde_json::json!({
                        "key": "view",
                        "message": format!("{VIEW} not found yet: the build ({command}) has to write it"),
                    }),
                );
            }
        }
        verdict
    }

    /// Loads the plugin in `dir`; `before_build` lets a declared build be
    /// the one to write the entry.
    fn try_load(dir: &Path, before_build: bool) -> Result<Plugin, String> {
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
        let unknown_keys = shape::unknown_keys(&manifest);

        // the schema has checked the shapes; what is left is what it cannot
        // say, such as whether the files named are there
        let name = manifest["name"].as_str().unwrap_or_default().to_string();
        let version = manifest["version"]
            .as_str()
            .and_then(line_of)
            .and_then(|_| Some(version_of(&manifest["version"])?.0));
        let version = version.ok_or("version is not a semantic version like \"1.2.0\"")?;
        // the oldest Pinrail the plugin says it works with
        if let Some(needed) = manifest.get("pinrail").and_then(Value::as_str) {
            let needed = needed.trim_start_matches(">=").trim();
            let this = env!("CARGO_PKG_VERSION");
            if crate::semver(this) < crate::semver(needed) {
                return Err(format!(
                    "the plugin needs Pinrail {needed} or later; this is Pinrail {this}"
                ));
            }
        }
        let builds = manifest
            .get("build")
            .and_then(|build| build["command"].as_str())
            .is_some_and(|command| !command.trim().is_empty());
        let found = dir.join(VIEW).is_file();
        if !found && !(before_build && builds) {
            return Err(format!("{VIEW} not found"));
        }
        // an icon that does not load costs the plugin its icon, not its place
        let (icon, icon_error) = if dir.join(ICON).exists() {
            match icon_markup(dir, ICON) {
                Ok(svg) => (Some(svg), None),
                Err(message) => (None, Some(message)),
            }
        } else {
            (None, None)
        };

        // the two schemas the plugin cannot do without, each in its place
        let schema = |key: &str, file: &str| {
            if !dir.join(file).is_file() {
                return Err(format!("{file} not found"));
            }
            Schema::compile(
                dir,
                &name,
                &version,
                key,
                &serde_json::json!({ "$ref": file }),
            )
        };
        let payload_schema = schema("payload_schema", PAYLOAD_SCHEMA)?;
        let decision_schema = schema("decision_schema", DECISION_SCHEMA)?;
        // a bad settings schema costs the plugin its settings, not its place
        let (settings_schema, settings_validator, settings_error) =
            match manifest.get("settings_schema") {
                _ if shape.dropped.contains_key("settings_schema") => {
                    (None, None, shape.dropped.get("settings_schema").cloned())
                }
                None | Some(Value::Null) => (None, None, None),
                Some(raw) => match settings::load(dir, &name, &version, raw) {
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
        let (decision_template, template_error) = if dir.join(TEMPLATE).exists() {
            match read_inside(dir, TEMPLATE) {
                Ok(source) => match crate::compile_template(&source) {
                    Ok(()) => (Some(source), None),
                    Err(message) => (None, Some(format!("{TEMPLATE}: {message}"))),
                },
                Err(e) => (None, Some(format!("{TEMPLATE}: cannot read ({e})"))),
            }
        } else {
            (None, None)
        };
        // a summary that does not read costs the plugin its summaries
        let (summary, summary_error) = match manifest.get("summary") {
            _ if shape.dropped.contains_key("summary") => {
                (Default::default(), shape.dropped.get("summary").cloned())
            }
            None | Some(Value::Null) => (Default::default(), None),
            Some(raw) => match crate::summary::Declaration::load(raw) {
                Ok(declaration) => (declaration, None),
                Err(message) => (Default::default(), Some(message)),
            },
        };
        // each sample that does not load is dropped, so what is sent and
        // what an agent is shown always submits
        let (samples, sample_errors) = crate::sample::load_all(dir, &payload_schema);
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
            path: dir.to_path_buf(),
            min_height: manifest
                .get("min_height")
                .and_then(Value::as_u64)
                .filter(|n| *n > 0)
                .map(|n| n as u32)
                .unwrap_or(400),
            dev: manifest.get("dev") == Some(&Value::Bool(true)),
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
            summary,
            summary_error,
            use_when,
            samples,
            sample_errors,
            unknown_keys,
            attachments,
            install: None,
            error: None,
        })
    }

    /// The payload an agent is shown to start from: the first sample's.
    pub fn example(&self) -> Option<&Value> {
        self.samples.first().map(|sample| &sample.payload)
    }

    /// The samples' names, in order: what `--sample <name>` takes.
    pub fn sample_names(&self) -> Vec<&str> {
        self.samples
            .iter()
            .map(|sample| sample.name.as_str())
            .collect()
    }

    /// The sample of this name, or the first when no name is given.
    pub fn sample(&self, name: Option<&str>) -> Option<&crate::sample::Sample> {
        match name {
            None => self.samples.first(),
            Some(name) => self.samples.iter().find(|sample| sample.name == name),
        }
    }

    pub fn usable(&self) -> bool {
        self.error.is_none()
    }

    /// Whether the manifest declares settings the core accepted.
    pub fn has_settings(&self) -> bool {
        self.settings_validator.is_some()
    }

    /// A change to the plugin's settings checked against its schema; the
    /// paths come back under `/plugins/<full name>`, its `/` spelled `~1`.
    pub fn validate_settings(&self, patch: &Value) -> Vec<Violation> {
        let prefix = format!(
            "/plugins/{}",
            self.full_name().replace('~', "~0").replace('/', "~1")
        );
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
    /// The plugin's full name, `<publisher>/<name>`, once it is installed;
    /// its name alone before.
    pub fn full_name(&self) -> &str {
        self.install
            .as_ref()
            .map(|i| i.plugin.as_str())
            .unwrap_or(&self.name)
    }

    pub fn describe(&self) -> Value {
        serde_json::json!({
            "plugin": self.full_name(),
            "name": self.name,
            "title": self.title,
            "version": self.version,
            "description": self.manifest.get("description"),
            "use_when": self.use_when,
            "payload_schema": self.schema_document("payload_schema"),
            "decision_schema": self.schema_document("decision_schema"),
            "example": self.example(),
            "samples": self.sample_names(),
            "attachments": self.manifest.get("attachments"),
            "markdown": self.decision_template.is_some(),
        })
    }

    /// The manifest's schema under `key`, or the file its `$ref` names when
    /// that is all it holds.
    fn schema_document(&self, key: &str) -> Value {
        let file = if key == "payload_schema" {
            PAYLOAD_SCHEMA
        } else {
            DECISION_SCHEMA
        };
        std::fs::read_to_string(self.path.join(file))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null)
    }

    /// What the app makes of the folder, as `pinrail-plugin check --json`
    /// says it: usable or not, why it would be refused, and each feature it
    /// would drop, keyed by the manifest key.
    pub fn verdict(&self) -> Value {
        let warnings: Vec<Value> = [
            ("settings_schema", &self.settings_error),
            ("shortcuts", &self.shortcuts_error),
            ("template", &self.template_error),
            ("summary", &self.summary_error),
            ("icon", &self.icon_error),
        ]
        .into_iter()
        .filter_map(|(key, error)| {
            error
                .as_ref()
                .map(|message| serde_json::json!({ "key": key, "message": message }))
        })
        .chain(
            self.sample_errors
                .iter()
                .map(|message| serde_json::json!({ "key": "samples", "message": message })),
        )
        .chain(self.unknown_keys.iter().map(|key| {
            let message = PLACED
                .iter()
                .find(|(placed, _)| placed == key)
                .map(|(_, place)| format!("no longer read: {place}"))
                .unwrap_or_else(|| UNKNOWN_KEY.to_string());
            serde_json::json!({ "key": key, "message": message })
        }))
        .collect();
        serde_json::json!({
            "usable": self.usable(),
            "name": self.usable().then_some(&self.name),
            "version": self.usable().then_some(&self.version),
            "problems": self.error.iter().map(|message| serde_json::json!({ "message": message })).collect::<Vec<_>>(),
            "warnings": if self.usable() { warnings } else { Vec::new() },
        })
    }

    /// What the API lists for a plugin.
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "plugin": self.full_name(),
            "publisher": self.install.as_ref().map(|i| &i.publisher),
            "name": self.name,
            "version": self.version,
            "title": self.title,
            "path": self.path.display().to_string(),
            "min_height": self.min_height,
            "dev": self.dev,
            "icon": self.icon,
            "icon_error": self.icon_error,
            "usable": self.usable(),
            "error": self.error,
            "payload_schema": self.schema_document("payload_schema"),
            "decision_schema": self.schema_document("decision_schema"),
            "settings_schema": self.settings_schema,
            "settings_error": self.settings_error,
            "shortcuts": self.shortcuts,
            "shortcuts_error": self.shortcuts_error,
            "template_error": self.template_error,
            "summary_error": self.summary_error,
            "description": self.manifest.get("description"),
            "use_when": self.use_when,
            "samples": self.sample_names(),
            "sample_errors": self.sample_errors,
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
pub use shape::{FEATURES, SCHEMA};

mod shape {
    use std::collections::BTreeMap;
    use std::sync::OnceLock;

    use serde_json::Value;

    use crate::schema::Schema;

    /// The schema's text, as the pinrail-plugin package ships it.
    pub const SCHEMA: &str = include_str!(concat!(env!("OUT_DIR"), "/manifest.schema.json"));

    /// Keys whose violation costs the plugin that feature, not its place.
    pub const FEATURES: &[&str] = &["settings_schema", "shortcuts", "summary"];

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

    /// The manifest's top-level keys that the schema does not define.
    pub fn unknown_keys(manifest: &serde_json::Map<String, Value>) -> Vec<String> {
        static KNOWN: OnceLock<Vec<String>> = OnceLock::new();
        let known = KNOWN.get_or_init(|| {
            let document: Value =
                serde_json::from_str(SCHEMA).expect("manifest.schema.json is JSON");
            document["properties"]
                .as_object()
                .map(|p| p.keys().cloned().collect())
                .unwrap_or_default()
        });
        manifest
            .keys()
            .filter(|k| !known.contains(k))
            .cloned()
            .collect()
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

    /// The order the shell writes a key press in, so a declared combination
    /// and a pressed one compare as text.
    const ORDER: &[&str] = &["ctrl", "alt", "shift", "cmd"];

    /// A modifier by its one name: `command`, `meta` and `super` are `cmd`,
    /// `option` is `alt`, `control` is `ctrl`, and `cmdorctrl` is the
    /// platform's own.
    fn modifier(name: &str) -> Option<&'static str> {
        Some(match name {
            "cmd" | "command" | "meta" | "super" => "cmd",
            "ctrl" | "control" => "ctrl",
            "alt" | "option" => "alt",
            "shift" => "shift",
            "cmdorctrl" | "commandorcontrol" if cfg!(target_os = "macos") => "cmd",
            "cmdorctrl" | "commandorcontrol" => "ctrl",
            _ => return None,
        })
    }

    /// Modifiers in any order, case and spelling, then one key; back in
    /// lowercase, with the modifiers in `ORDER`.
    fn normalize(keys: &str) -> Option<String> {
        let parts: Vec<String> = keys.split('+').map(|p| p.trim().to_lowercase()).collect();
        let (key, modifiers) = parts.split_last()?;
        if key.is_empty() || key.chars().any(char::is_whitespace) {
            return None;
        }
        let named = modifiers
            .iter()
            .map(|m| modifier(m))
            .collect::<Option<Vec<_>>>()?;
        let mut out: Vec<&str> = ORDER
            .iter()
            .copied()
            .filter(|m| named.contains(m))
            .collect();
        out.push(key);
        Some(out.join("+"))
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
        version: &str,
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

pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// A file of the plugin's, read only when it lies inside its folder, links
/// followed.
fn read_inside(dir: &Path, file: &str) -> std::io::Result<String> {
    let path = crate::schema::safe_join(dir, file).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "it is outside the plugin's folder",
        )
    })?;
    std::fs::read_to_string(path)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn the_line_of_a_version_is_its_major_or_below_one_its_minor() {
        for (version, line) in [
            ("1.4.2", "1"),
            ("2.0.0", "2"),
            ("10.3.7", "10"),
            ("0.3.1", "0.3"),
            ("0.12.0", "0.12"),
            ("0.0.4", "0.0"),
        ] {
            assert_eq!(line_of(version).as_deref(), Some(line), "{version}");
        }
        for version in ["1.4", "1", "", "v1.0.0", "1.0.0-beta", "1.x.0", "1.0.0.0"] {
            assert_eq!(line_of(version), None, "{version}");
        }
    }

    /// A plugin reads only files inside its folder: an entry that climbs
    /// out is refused, and a template or an example that a link carries
    /// out of the folder is dropped.
    #[cfg(unix)]
    #[cfg(unix)]
    #[test]
    fn a_plugin_reads_no_file_outside_its_folder() {
        let root = tempfile::tempdir().unwrap();
        let outside = root.path().join("other");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("t.j2"), "{{ note }}").unwrap();
        std::fs::write(outside.join("s.json"), r#"{"title": "t", "payload": {}}"#).unwrap();
        let linked = with_manifest(root.path(), "linked", manifest(serde_json::json!({})));
        std::fs::create_dir_all(linked.join("templates")).unwrap();
        std::fs::create_dir_all(linked.join("samples")).unwrap();
        std::os::unix::fs::symlink(outside.join("t.j2"), linked.join(TEMPLATE)).unwrap();
        std::os::unix::fs::symlink(outside.join("s.json"), linked.join("samples/s.json")).unwrap();
        let loaded = Plugin::load(&linked);
        assert!(loaded.error.is_none(), "{:?}", loaded.error);
        assert!(
            loaded.decision_template.is_none(),
            "a template outside the folder was read"
        );
        assert!(
            loaded.samples.is_empty(),
            "a sample outside the folder was read"
        );
    }

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
        let dir = layout(root, name);
        std::fs::write(
            dir.join("manifest.json"),
            format!("{{\"name\":\"{name}\",\"version\":\"1.0.0\"{extra}}}"),
        )
        .unwrap();
        dir
    }

    /// A plugin folder in the fixed layout with what every plugin must have
    /// but its manifest: a view and two schemas that take anything.
    fn layout(root: &Path, folder: &str) -> PathBuf {
        let dir = root.join(folder);
        std::fs::create_dir_all(dir.join("view")).unwrap();
        std::fs::create_dir_all(dir.join("schemas")).unwrap();
        std::fs::write(dir.join(VIEW), "<html></html>").unwrap();
        std::fs::write(dir.join(PAYLOAD_SCHEMA), "{}").unwrap();
        std::fs::write(dir.join(DECISION_SCHEMA), "{}").unwrap();
        dir
    }

    /// A plugin folder with exactly this manifest, a view and two schemas.
    fn with_manifest(root: &Path, folder: &str, manifest: serde_json::Value) -> PathBuf {
        let dir = layout(root, folder);
        std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
        dir
    }

    /// The smallest manifest the schema accepts, with `changes` laid over
    /// it; a null in `changes` removes the key.
    fn manifest(changes: serde_json::Value) -> serde_json::Value {
        let mut m = serde_json::json!({"name": "sample", "version": "1.0.0"});
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
        let tmp = tempfile::tempdir().unwrap();
        let ok = with_manifest(tmp.path(), "ok", manifest(serde_json::json!({})));
        std::fs::write(
            ok.join(ICON),
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

        // no icon.svg is no icon, and no complaint
        let none = Plugin::load(&with_manifest(
            tmp.path(),
            "none",
            manifest(serde_json::json!({})),
        ));
        assert!(none.icon.is_none() && none.icon_error.is_none());

        // an icon.svg that is not an SVG, or not a file, costs only the icon
        let not_svg = with_manifest(tmp.path(), "not-svg", manifest(serde_json::json!({})));
        std::fs::write(not_svg.join(ICON), "hello").unwrap();
        let folder = with_manifest(tmp.path(), "folder", manifest(serde_json::json!({})));
        std::fs::create_dir_all(folder.join(ICON)).unwrap();
        for (dir, why) in [
            (not_svg, "icon.svg: not an SVG"),
            (folder, "icon.svg: cannot read"),
        ] {
            let p = Plugin::load(&dir);
            assert!(p.usable(), "{}: {:?}", dir.display(), p.error);
            assert!(p.icon.is_none());
            let error = p.icon_error.clone().unwrap_or_default();
            assert!(error.starts_with(why), "{}: {error}", dir.display());
        }
    }

    #[test]
    fn a_manifest_that_breaks_its_schema_is_refused_and_says_where() {
        use serde_json::json;
        let tmp = tempfile::tempdir().unwrap();
        let cases = [
            (json!({"name": null}), "property 'name' is required"),
            (json!({"version": null}), "property 'version' is required"),
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
        // and the files the schema cannot see: the view and the two schemas
        // must be in their places, and the schemas must be schemas
        for (i, (file, text, expected)) in [
            (VIEW, None, "view/index.html not found"),
            (
                PAYLOAD_SCHEMA,
                None,
                "schemas/payload.schema.json not found",
            ),
            (
                DECISION_SCHEMA,
                None,
                "schemas/decision.schema.json not found",
            ),
            (PAYLOAD_SCHEMA, Some("{"), "payload_schema"),
            (DECISION_SCHEMA, Some("{\"type\": 3}"), "decision_schema"),
        ]
        .into_iter()
        .enumerate()
        {
            let dir = with_manifest(tmp.path(), &format!("files{i}"), manifest(json!({})));
            match text {
                None => std::fs::remove_file(dir.join(file)).unwrap(),
                Some(text) => std::fs::write(dir.join(file), text).unwrap(),
            }
            let error = Plugin::load(&dir).error.unwrap_or_default();
            assert!(error.contains(expected), "{file}: {error}");
        }
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
            (json!({"summary": 3}), "summary", "summary: "),
            (
                json!({"summary": {"request": {"counts": [{"items": "/a", "label": "x", "tone": "red"}]}}}),
                "summary",
                "summary/request/counts/0: tone",
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
                "summary" => p.summary_error.clone(),
                _ => p.template_error.clone(),
            }
            .unwrap_or_default();
            assert!(why.starts_with(expected), "{changes}: {why}");
        }

        // a template that does not compile costs the plugin its template
        let dir = with_manifest(tmp.path(), "template", manifest(json!({})));
        std::fs::create_dir_all(dir.join("templates")).unwrap();
        std::fs::write(dir.join(TEMPLATE), "{% if %}").unwrap();
        let p = Plugin::load(&dir);
        assert!(p.usable(), "{:?}", p.error);
        assert!(p.decision_template.is_none());
        let why = p.template_error.unwrap_or_default();
        assert!(why.starts_with("templates/decision.md.j2: "), "{why}");
    }

    #[test]
    fn the_example_is_the_first_samples_payload_and_use_when_is_read() {
        use serde_json::json;
        let tmp = tempfile::tempdir().unwrap();
        let dir = with_manifest(
            tmp.path(),
            "described",
            manifest(json!({"use_when": "Before posting"})),
        );
        std::fs::write(
            dir.join(PAYLOAD_SCHEMA),
            r#"{"type": "object", "required": ["n"]}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("samples")).unwrap();
        std::fs::write(
            dir.join("samples/one.json"),
            r#"{"title": "One", "payload": {"n": 1}}"#,
        )
        .unwrap();
        let p = Plugin::load(&dir);
        assert_eq!(p.use_when.as_deref(), Some("Before posting"));
        assert_eq!(p.example(), Some(&json!({"n": 1})));
        assert_eq!(p.describe()["example"], json!({"n": 1}));
        assert_eq!(p.describe()["samples"], json!(["one"]));
        assert_eq!(p.to_json()["samples"], json!(["one"]));

        // a sample that does not pass is no example, and says why
        std::fs::write(
            dir.join("samples/one.json"),
            r#"{"title": "One", "payload": {"m": 1}}"#,
        )
        .unwrap();
        let p = Plugin::load(&dir);
        assert_eq!(p.example(), None);
        let warning = &p.verdict()["warnings"][0];
        assert_eq!(warning["key"], "samples");
        assert!(
            warning["message"]
                .as_str()
                .unwrap()
                .contains("does not pass payload_schema"),
            "{warning}"
        );
    }

    /// A plugin can say which Pinrail it needs; an app older than that
    /// refuses it and says which version to install.
    #[test]
    fn a_plugin_that_needs_a_newer_pinrail_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Plugin::load(&plugin_dir(tmp.path(), "future", r#","pinrail":">=99.1""#));
        let why = p.error.clone().unwrap_or_default();
        assert!(why.contains("needs Pinrail 99.1 or later"), "{why}");
        assert!(why.contains(env!("CARGO_PKG_VERSION")), "{why}");

        let p = Plugin::load(&plugin_dir(tmp.path(), "present", r#","pinrail":">=0.1""#));
        assert_eq!(p.error, None);

        for bad in [
            r#","pinrail":"0.1""#,
            r#","pinrail":"latest""#,
            r#","pinrail":2"#,
        ] {
            let p = Plugin::load(&plugin_dir(tmp.path(), "bad", bad));
            assert!(
                p.error.as_deref().is_some_and(|e| e.contains("pinrail")),
                "{bad}: {:?}",
                p.error
            );
        }
    }

    /// A key the manifest schema does not define is kept, and warned about:
    /// a typo, or a key a newer Pinrail reads.
    #[test]
    fn an_unknown_manifest_key_is_warned_about() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Plugin::load(&plugin_dir(tmp.path(), "typo", r#","colour":"red""#));
        assert_eq!(p.error, None);
        let warnings = p.verdict()["warnings"].clone();
        assert!(
            warnings
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["key"] == "colour"),
            "{warnings}"
        );
    }

    /// A key that once named a file still loads the plugin, and says where
    /// the file always is now.
    #[test]
    fn a_key_that_named_a_file_says_where_the_file_is_now() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Plugin::load(&plugin_dir(
            tmp.path(),
            "older",
            r#","entry":"index.html","icon":"mark.svg","payload_schema":{},"decision_schema":{"$ref":"d.json"},"decision_template":"t.j2""#,
        ));
        assert_eq!(p.error, None);
        let warnings: Vec<(String, String)> = p.verdict()["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| {
                (
                    w["key"].as_str().unwrap().into(),
                    w["message"].as_str().unwrap().into(),
                )
            })
            .collect();
        assert_eq!(
            warnings,
            [
                (
                    "entry",
                    "no longer read: the view is always view/index.html"
                ),
                ("icon", "no longer read: the icon is always icon.svg"),
                (
                    "payload_schema",
                    "no longer read: the payload schema is always schemas/payload.schema.json"
                ),
                (
                    "decision_schema",
                    "no longer read: the decision schema is always schemas/decision.schema.json"
                ),
                (
                    "decision_template",
                    "no longer read: the template is always templates/decision.md.j2"
                ),
            ]
            .map(|(k, m)| (k.to_string(), m.to_string()))
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
        assert_eq!(p.version, "0.1.0");
        let p = with("\"2.3.4\"");
        assert_eq!(p.version, "2.3.4");
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
        let end = start + text[start..].find([',', '}']).unwrap();
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
        let good = r#","shortcuts":[{"keys":"j","does":"Next"},{"keys":" Cmd + Shift+M ","does":"Maximize","group":"View"},{"keys":"shift+/","does":"Help"},{"keys":"Option+Command+Control+x","does":"Synonyms"},{"keys":"cmdorctrl+k","does":"Platform"}]"#;
        let p = Plugin::load(&plugin_dir(tmp.path(), "keys", good));
        assert!(p.usable(), "{:?}", p.error);
        assert_eq!(p.shortcuts_error, None);
        assert_eq!(
            p.shortcuts,
            vec![
                // modifiers in the order a key press is read: ctrl, alt, shift, cmd
                serde_json::json!({"keys": "j", "does": "Next"}),
                serde_json::json!({"keys": "shift+cmd+m", "does": "Maximize", "group": "View"}),
                serde_json::json!({"keys": "shift+/", "does": "Help"}),
                serde_json::json!({"keys": "ctrl+alt+cmd+x", "does": "Synonyms"}),
                serde_json::json!({"keys": if cfg!(target_os = "macos") { "cmd+k" } else { "ctrl+k" }, "does": "Platform"}),
            ]
        );
        assert_eq!(p.to_json()["shortcuts"][1]["keys"], "shift+cmd+m");

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
