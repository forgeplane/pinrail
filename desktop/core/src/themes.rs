//! Themes: the bundled ones and the JSON files in the themes folder.
//!
//! A theme belongs to the dark or the light colour scheme and sets colours
//! by the keys in [`KEYS`]. What it leaves out comes from the built-in theme
//! of its scheme, whose colours the stylesheets already hold, so a theme is
//! an overlay of CSS variables. This module turns a theme into those
//! variables, one map for the shell and one for plugin views.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use include_dir::{Dir, include_dir};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::db::Db;
use crate::error::Error;
use crate::events::{self, Bus, Notice};

static BUNDLED: Dir = include_dir!("$CARGO_MANIFEST_DIR/themes");

/// The themes whose colours are the stylesheets' own. Their ids are reserved.
const BUILT_IN: [&str; 2] = ["dark", "light"];
const MAX_FILE_BYTES: u64 = 64 * 1024;
const MAX_NAME_CHARS: usize = 60;
const MAX_ID_CHARS: usize = 40;
const MAX_COLOR_CHARS: usize = 200;
const COLOR_FUNCTIONS: [&str; 6] = ["rgb", "rgba", "hsl", "hsla", "oklch", "color-mix"];

/// Each colour key, the shell's variables and the SDK's variables for it.
const KEYS: &[(&str, &[&str], &[&str])] = &[
    ("bg", &["--bg"], &["--pinrail-bg"]),
    ("bg-panel", &["--bg-panel"], &["--pinrail-bg-panel"]),
    ("bg-raised", &["--bg-raised"], &["--pinrail-bg-raised"]),
    ("bg-hover", &["--bg-hover"], &["--pinrail-bg-hover"]),
    ("sidebar", &["--sidebar"], &[]),
    ("border", &["--border"], &["--pinrail-border"]),
    (
        "border-strong",
        &["--border-strong"],
        &["--pinrail-border-strong"],
    ),
    ("text", &["--text"], &["--pinrail-text"]),
    ("dim", &["--dim"], &["--pinrail-dim"]),
    ("faint", &["--faint"], &["--pinrail-faint"]),
    (
        "accent",
        &["--accent"],
        &["--pinrail-accent", "--pinrail-info"],
    ),
    ("accent-bg", &["--accent-bg"], &["--pinrail-accent-bg"]),
    ("button-bg", &["--button-bg"], &["--pinrail-button-bg"]),
    (
        "button-text",
        &["--button-text"],
        &["--pinrail-button-text"],
    ),
    (
        "danger",
        &["--danger", "--sev-blocker"],
        &["--pinrail-danger"],
    ),
    ("warning", &["--sev-major"], &["--pinrail-warning"]),
    ("caution", &["--sev-minor"], &[]),
    ("success", &["--ok"], &["--pinrail-success"]),
    ("neutral", &["--sev-nit"], &["--pinrail-neutral"]),
    ("tooltip-bg", &["--tip-bg"], &[]),
    ("tooltip-text", &["--tip-text"], &[]),
    ("tooltip-key-bg", &["--tip-key-bg"], &[]),
    ("tooltip-key-text", &["--tip-key-text"], &[]),
    ("tooltip-danger-bg", &["--tip-danger-bg"], &[]),
    ("tooltip-danger-text", &["--tip-danger-text"], &[]),
    ("diff-add-bg", &[], &["--pinrail-add-bg"]),
    ("diff-add-gutter", &[], &["--pinrail-add-gut"]),
    ("diff-del-bg", &[], &["--pinrail-del-bg"]),
    ("diff-del-gutter", &[], &["--pinrail-del-gut"]),
    (
        "code-keyword",
        &["--code-keyword"],
        &["--pinrail-code-keyword"],
    ),
    (
        "code-string",
        &["--code-string"],
        &["--pinrail-code-string"],
    ),
    (
        "code-number",
        &["--code-number"],
        &["--pinrail-code-number"],
    ),
    (
        "code-comment",
        &["--code-comment"],
        &["--pinrail-code-comment"],
    ),
    ("code-title", &["--code-title"], &["--pinrail-code-title"]),
    ("code-type", &["--code-type"], &["--pinrail-code-type"]),
    ("code-attr", &["--code-attr"], &["--pinrail-code-attr"]),
    (
        "code-symbol",
        &["--code-symbol"],
        &["--pinrail-code-symbol"],
    ),
    ("code-meta", &["--code-meta"], &["--pinrail-code-meta"]),
];

/// A theme as the API lists it. An invalid file has an `error`, no scheme and
/// no variables, and cannot be applied.
#[derive(Debug, Clone, Serialize)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub scheme: Option<String>,
    /// `bundled`, or `user` for a file in the themes folder
    pub source: &'static str,
    /// The CSS variables for the shell, empty for a built-in theme.
    pub shell: BTreeMap<String, String>,
    /// The CSS variables for plugin views, empty for a built-in theme.
    pub plugin: BTreeMap<String, String>,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

/// A valid theme file.
struct Parsed {
    name: String,
    scheme: String,
    colors: BTreeMap<String, String>,
    warnings: Vec<String>,
}

/// One entry of the themes folder, to notice a change to it.
type Stamp = (PathBuf, Option<SystemTime>, u64);

#[derive(Debug)]
pub struct Themes {
    dir: PathBuf,
    db: Arc<Db>,
    bus: Bus,
    seen: Mutex<Vec<Stamp>>,
}

impl Themes {
    /// Opens the themes folder, creating it so that a person can find it.
    pub(crate) fn open(dir: &Path, db: Arc<Db>, bus: Bus) -> Result<Self, Error> {
        fs::create_dir_all(dir).map_err(Error::Io)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            db,
            bus,
            seen: Mutex::new(stamps(dir)),
        })
    }

    /// Every theme: the built-in ones, then the others by name. A person's
    /// file replaces a bundled theme with the same id.
    pub fn list(&self) -> Vec<Theme> {
        let mut themes: Vec<Theme> = self.user_themes();
        for file in BUNDLED.files() {
            let id = bundled_id(file.path());
            if !themes.iter().any(|t| t.id == id) || BUILT_IN.contains(&id) {
                let text = file.contents_utf8().expect("a bundled theme is UTF-8");
                let parsed = parse(text).expect("a bundled theme is valid");
                themes.push(resolved(id, "bundled", parsed));
            }
        }
        themes.sort_by_cached_key(|t| {
            (
                !(t.source == "bundled" && BUILT_IN.contains(&t.id.as_str())),
                t.name.to_lowercase(),
            )
        });
        themes
    }

    /// Writes a new theme with every colour that the theme `from` shows, and
    /// returns the file's path.
    pub fn create(&self, id: &str, from: &str) -> Result<PathBuf, Error> {
        if !valid_id(id) {
            return Err(Error::invalid(
                "/id",
                "must be lowercase letters, digits and hyphens",
            ));
        }
        if BUILT_IN.contains(&id) {
            return Err(Error::invalid("/id", "is reserved"));
        }
        let source = self
            .read(from)
            .ok_or_else(|| Error::NotFound(format!("theme {from}")))?;
        let base = bundled(&source.scheme).expect("each scheme has a built-in theme");
        let mut colors = Map::new();
        for (key, _, _) in KEYS {
            let value = source.colors.get(*key).or_else(|| base.colors.get(*key));
            if let Some(value) = value {
                colors.insert((*key).to_string(), json!(value));
            }
        }
        let theme = json!({ "name": title(id), "scheme": source.scheme, "colors": colors });
        let text = serde_json::to_string_pretty(&theme).expect("a theme is JSON");

        fs::create_dir_all(&self.dir).map_err(Error::Io)?;
        let path = self.dir.join(format!("{id}.json"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| match error.kind() {
                ErrorKind::AlreadyExists => Error::Conflict(format!("theme {id} already exists")),
                _ => Error::Io(error),
            })?;
        file.write_all(format!("{text}\n").as_bytes())
            .map_err(Error::Io)?;
        Ok(path)
    }

    /// Announces a change to the themes folder made since the last call.
    /// The host decides when to poll. Returns whether anything changed.
    pub fn reload(&self) -> Result<bool, Error> {
        let now = stamps(&self.dir);
        {
            let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
            if *seen == now {
                return Ok(false);
            }
            *seen = now;
        }
        let event_id = self
            .db
            .append_event(None, events::THEMES_CHANGED, None, &json!({}))?;
        self.bus
            .publish(Notice::new(event_id, events::THEMES_CHANGED, &Value::Null));
        Ok(true)
    }

    /// A valid theme by id, the person's file before a bundled one.
    fn read(&self, id: &str) -> Option<Parsed> {
        if !BUILT_IN.contains(&id) && valid_id(id) {
            let user = read_file(&self.dir.join(format!("{id}.json")));
            if let Ok(parsed) = user {
                return Some(parsed);
            }
        }
        bundled(id)
    }

    fn user_themes(&self) -> Vec<Theme> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|entry| is_theme_file(&entry.path()))
            .map(|entry| {
                let path = entry.path();
                let id = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let parsed = if !valid_id(&id) {
                    Err("The file name must be lowercase letters, digits and hyphens.".to_string())
                } else if BUILT_IN.contains(&id.as_str()) {
                    Err(format!("The id \"{id}\" is reserved."))
                } else {
                    read_file(&path)
                };
                match parsed {
                    Ok(parsed) => resolved(&id, "user", parsed),
                    Err(error) => Theme {
                        name: id.clone(),
                        id,
                        scheme: None,
                        source: "user",
                        shell: BTreeMap::new(),
                        plugin: BTreeMap::new(),
                        warnings: Vec::new(),
                        error: Some(error),
                    },
                }
            })
            .collect()
    }
}

/// Whether a string can be a theme's id.
pub fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id.len() <= MAX_ID_CHARS
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_theme_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "json") && path.is_file()
}

fn stamps(dir: &Path) -> Vec<Stamp> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<Stamp> = entries
        .flatten()
        .filter(|entry| is_theme_file(&entry.path()))
        .map(|entry| {
            let meta = entry.metadata().ok();
            (
                entry.path(),
                meta.as_ref().and_then(|m| m.modified().ok()),
                meta.map_or(0, |m| m.len()),
            )
        })
        .collect();
    found.sort();
    found
}

fn bundled_id(path: &Path) -> &str {
    path.file_stem()
        .and_then(|s| s.to_str())
        .expect("a bundled theme has a UTF-8 name")
}

/// Whether `id` is one of the themes the app carries.
pub(crate) fn is_bundled(id: &str) -> bool {
    BUNDLED.get_file(format!("{id}.json")).is_some()
}

fn bundled(id: &str) -> Option<Parsed> {
    let file = BUNDLED.get_file(format!("{id}.json"))?;
    parse(file.contents_utf8()?).ok()
}

fn read_file(path: &Path) -> Result<Parsed, String> {
    let meta = fs::metadata(path).map_err(|e| format!("The file cannot be read: {e}."))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err("The file is larger than 64 KiB.".to_string());
    }
    let text = fs::read_to_string(path).map_err(|e| format!("The file cannot be read: {e}."))?;
    parse(&text)
}

fn parse(text: &str) -> Result<Parsed, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("The file is not valid JSON: {e}."))?;
    let object = value
        .as_object()
        .ok_or("The file must hold a JSON object.")?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty() && name.chars().count() <= MAX_NAME_CHARS)
        .ok_or("`name` must be text of 1 to 60 characters.")?;
    let scheme = object
        .get("scheme")
        .and_then(Value::as_str)
        .filter(|scheme| BUILT_IN.contains(scheme))
        .ok_or("`scheme` must be \"dark\" or \"light\".")?;

    let mut warnings: Vec<String> = object
        .keys()
        .filter(|key| !["name", "scheme", "colors"].contains(&key.as_str()))
        .map(|key| format!("Unknown field `{key}`."))
        .collect();
    let mut colors = BTreeMap::new();
    match object.get("colors") {
        None => {}
        Some(Value::Object(map)) => {
            for (key, value) in map {
                if !KEYS.iter().any(|(known, _, _)| known == key) {
                    warnings.push(format!("Unknown colour key `{key}`."));
                    continue;
                }
                let color = value
                    .as_str()
                    .filter(|color| valid_color(color))
                    .ok_or(format!("`{key}` is not a valid colour."))?;
                colors.insert(key.clone(), color.trim().to_string());
            }
        }
        Some(_) => return Err("`colors` must be an object.".to_string()),
    }
    Ok(Parsed {
        name: name.to_string(),
        scheme: scheme.to_string(),
        colors,
        warnings,
    })
}

/// A hex colour or a colour function. A value reaches the shell and plugin
/// frames, and a theme may have been downloaded, so nothing else gets through.
fn valid_color(value: &str) -> bool {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        return [3, 4, 6, 8].contains(&hex.len()) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    let allowed = |c: char| c.is_ascii_alphanumeric() || " .,%()/#+-".contains(c);
    if value.is_empty()
        || value.len() > MAX_COLOR_CHARS
        || !value.ends_with(')')
        || !value.chars().all(allowed)
    {
        return false;
    }
    let mut calls = value.split_inclusive('(').filter(|p| p.ends_with('('));
    let known = |part: &str| {
        let name = part
            .trim_end_matches('(')
            .rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '-')
            .next()
            .unwrap_or_default();
        COLOR_FUNCTIONS.contains(&name)
    };
    COLOR_FUNCTIONS
        .iter()
        .any(|f| value.starts_with(&format!("{f}(")))
        && calls.all(known)
}

fn resolved(id: &str, source: &'static str, parsed: Parsed) -> Theme {
    let mut shell = BTreeMap::new();
    let mut plugin = BTreeMap::new();
    if !(source == "bundled" && BUILT_IN.contains(&id)) {
        for (key, shell_vars, plugin_vars) in KEYS {
            let Some(color) = parsed.colors.get(*key) else {
                continue;
            };
            for var in *shell_vars {
                shell.insert((*var).to_string(), color.clone());
            }
            for var in *plugin_vars {
                plugin.insert((*var).to_string(), color.clone());
            }
        }
    }
    Theme {
        id: id.to_string(),
        name: parsed.name,
        scheme: Some(parsed.scheme),
        source,
        shell,
        plugin,
        warnings: parsed.warnings,
        error: None,
    }
}

/// The name a new theme starts with: `my-theme` is "My theme".
fn title(id: &str) -> String {
    let words = id.replace('-', " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => words,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The variables of a stylesheet's dark and light blocks.
    fn palettes(css: &str) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        let block = |start: &str| {
            let rest = &css[css.find(start).expect("the block is there")..];
            let block = &rest[..rest.find('}').expect("the block ends")];
            block
                .split(';')
                .filter_map(|line| {
                    let at = line.find("--")?;
                    let (name, value) = line[at..].split_once(':')?;
                    Some((name.trim().to_string(), value.trim().to_string()))
                })
                .collect()
        };
        (block(":root {"), block("[data-theme=\"light\"] {"))
    }

    fn stylesheet(path: &str) -> String {
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
    }

    /// `dark.json` and `light.json` hold what the stylesheets paint, and the
    /// key table reaches every colour variable in them.
    #[test]
    fn the_built_in_themes_match_the_stylesheets() {
        let shell = palettes(&stylesheet("../app/ui/src/theme.css"));
        let sdk = palettes(&stylesheet("../../sdk/src/tokens.css"));
        for (id, shell, sdk) in [("dark", &shell.0, &sdk.0), ("light", &shell.1, &sdk.1)] {
            let theme = bundled(id).unwrap();
            assert_eq!(theme.colors.len(), KEYS.len(), "{id} sets every key");
            for (key, shell_vars, sdk_vars) in KEYS {
                let color = &theme.colors[*key];
                for var in shell_vars.iter() {
                    assert_eq!(shell.get(*var), Some(color), "{id}: {var}");
                }
                for var in sdk_vars.iter() {
                    assert_eq!(sdk.get(*var), Some(color), "{id}: {var}");
                }
            }
            let mapped = |var: &str| {
                ["--font", "--mono", "--pinrail-sans", "--pinrail-mono"].contains(&var)
                    || KEYS
                        .iter()
                        .any(|(_, a, b)| a.contains(&var) || b.contains(&var))
            };
            for var in shell.keys().chain(sdk.keys()) {
                assert!(mapped(var.as_str()), "{var} has no colour key");
            }
        }
    }

    #[test]
    fn every_bundled_theme_sets_every_key() {
        for file in BUNDLED.files() {
            let id = bundled_id(file.path());
            assert!(valid_id(id), "{id}");
            let theme = parse(file.contents_utf8().unwrap()).unwrap();
            assert!(theme.warnings.is_empty(), "{id}: {:?}", theme.warnings);
            assert_eq!(theme.colors.len(), KEYS.len(), "{id}");
        }
    }

    #[test]
    fn colours_are_checked() {
        for good in [
            "#abc",
            "#aabbcc80",
            "rgb(1 2 3 / 0.5)",
            "hsl(120, 50%, 50%)",
            "oklch(0.65 0.12 150 / 0.1)",
            "color-mix(in srgb, #fff 4%, rgb(0 0 0))",
        ] {
            assert!(valid_color(good), "{good}");
        }
        for bad in [
            "red",
            "#12",
            "#ggg",
            "url(http://x/y)",
            "color-mix(in srgb, url(x) 4%, #000)",
            "rgb(0 0 0); background: red",
            "var(--bg)",
            "rgb(var(--x))",
            "\"#fff\"",
            "",
        ] {
            assert!(!valid_color(bad), "{bad}");
        }
    }

    #[test]
    fn ids_are_checked() {
        assert!(valid_id("nord-2"));
        for bad in ["", "-a", "Nord", "a b", "../x", &"a".repeat(41)] {
            assert!(!valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn a_new_theme_is_named_after_its_id() {
        assert_eq!(title("my-theme"), "My theme");
    }
}
