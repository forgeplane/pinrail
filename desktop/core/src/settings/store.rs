//! `settings.json` in the data directory: what a person can change about the
//! app. The core owns it — defaults, validation, an atomic write — and
//! notices an edit made outside the app. Unknown keys in the file are kept
//! as they are, so a newer file survives an older app.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use serde_json::{Map, Value, json};

use super::PLUGINS;
use crate::error::{Error, Violation};

const FILE: &str = "settings.json";

/// What a leaf may hold.
#[derive(Debug, Clone, Copy)]
enum Kind {
    Bool,
    Enum(&'static [&'static str]),
    /// a non-empty string
    Text,
    /// an RFC 3339 timestamp, or null
    NullableTime,
    /// a list of non-empty strings
    TextList,
    /// {from: "HH:MM", to: "HH:MM"}, or null
    NullableHours,
    Port,
    /// a positive integer, or null
    NullableDays,
}

/// A setting: its JSON pointer, its kind, its default, and one line saying
/// what it does, for the settings reference in the docs.
type Leaf = (&'static str, Kind, fn() -> Value, &'static str);

/// Every setting, its kind, its default and what it does. Paths are JSON
/// pointers.
const LEAVES: &[Leaf] = &[
    (
        "/appearance/theme",
        Kind::Enum(&["system", "dark", "light"]),
        || json!("system"),
        "The theme of the app and of every plugin view. `system` follows the operating system.",
    ),
    (
        "/appearance/text_size",
        Kind::Enum(&["small", "default", "large"]),
        || json!("default"),
        "The size of the interface's text.",
    ),
    (
        "/autostart",
        Kind::Bool,
        || json!(false),
        "Start Pinrail when you log in.",
    ),
    (
        "/close_window",
        Kind::Enum(&["hide", "quit"]),
        || json!("hide"),
        "What closing the window does: hide it and keep running in the menu bar, or quit.",
    ),
    (
        "/menu_bar_icon",
        Kind::Bool,
        || json!(true),
        "Show the menu bar icon, with the number of waiting reviews.",
    ),
    (
        "/sidebar/open",
        Kind::Bool,
        || json!(true),
        "Whether the sidebar is open. The app keeps it, so a new window matches the last.",
    ),
    (
        "/notifications/enabled",
        Kind::Bool,
        || json!(true),
        "Announce new reviews with a system notification.",
    ),
    (
        "/notifications/paused_until",
        Kind::NullableTime,
        || Value::Null,
        "No notifications until this time. The menu bar and Settings pause them for a while.",
    ),
    (
        "/notifications/sound",
        Kind::Bool,
        || json!(true),
        "Play the system sound with each notification.",
    ),
    (
        "/notifications/muted_plugins",
        Kind::TextList,
        || json!([]),
        "Plugins whose reviews are not announced. They are still counted in the menu bar.",
    ),
    (
        "/notifications/quiet_hours",
        Kind::NullableHours,
        || Value::Null,
        "No notifications between `from` and `to`, local time. A review that arrives then is not announced later.",
    ),
    (
        "/shortcut/global",
        Kind::Text,
        || json!("alt+shift+w"),
        "The shortcut that brings Pinrail forward from any app.",
    ),
    (
        "/shortcut/global_opens",
        Kind::Enum(&["oldest", "inbox"]),
        || json!("oldest"),
        "What the global shortcut opens: the oldest pending review, or the inbox.",
    ),
    (
        "/port",
        Kind::Port,
        || json!(4747),
        "The port of the server on 127.0.0.1. Takes effect after a restart; the CLI follows it.",
    ),
    (
        "/history/keep_days",
        Kind::NullableDays,
        || Value::Null,
        "Delete ended reviews older than this many days. `null` keeps them forever.",
    ),
    (
        "/updates/check",
        Kind::Bool,
        || json!(true),
        "Look for a new version of Pinrail at start and every few hours, and download it in the background. It is installed when Pinrail restarts.",
    ),
    (
        "/welcome/seen",
        Kind::Bool,
        || json!(false),
        "Whether the welcome screen has been closed. While `false`, Pinrail opens on it.",
    ),
];

/// Every setting as the reference shows it: dotted key, what it holds, its
/// default as JSON, and what it does.
#[cfg(feature = "docs")]
pub fn reference() -> Vec<(String, String, String, &'static str)> {
    LEAVES
        .iter()
        .map(|(path, kind, default, does)| {
            let key = path.trim_start_matches('/').replace('/', ".");
            (key, kind.describe(), default().to_string(), *does)
        })
        .collect()
}

#[cfg(feature = "docs")]
impl Kind {
    /// What a value of this kind looks like, in words.
    fn describe(&self) -> String {
        match self {
            Kind::Bool => "`true` or `false`".into(),
            Kind::Enum(values) => values
                .iter()
                .map(|v| format!("`\"{v}\"`"))
                .collect::<Vec<_>>()
                .join(", "),
            Kind::Text => "a non-empty string".into(),
            Kind::NullableTime => "an RFC 3339 time, or `null`".into(),
            Kind::TextList => "a list of non-empty strings".into(),
            Kind::NullableHours => "`{\"from\": \"HH:MM\", \"to\": \"HH:MM\"}`, or `null`".into(),
            Kind::Port => "a port, 1 to 65535".into(),
            Kind::NullableDays => "a positive number of days, or `null`".into(),
        }
    }
}

/// Every setting at its default.
fn defaults() -> Value {
    let mut out = Value::Object(Map::new());
    for (path, _, default, _) in LEAVES {
        set_at(&mut out, path, default());
    }
    set_at(&mut out, PLUGINS, Value::Object(Map::new()));
    out
}

/// The port the file asks for, read before anything else is open; `None`
/// when there is no file or it says nothing usable.
pub fn port_in(data_dir: &Path) -> Option<u16> {
    let text = std::fs::read_to_string(data_dir.join(FILE)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value
        .get("port")?
        .as_u64()
        .filter(|p| (1..=65535).contains(p))
        .map(|p| p as u16)
}

#[derive(Debug)]
pub(super) struct Store {
    path: PathBuf,
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    /// the file's contents, unknown keys included; never the defaults
    file: Value,
    /// what the file looked like when we last read or wrote it
    seen: Option<(SystemTime, u64)>,
}

impl Store {
    /// Opens `settings.json` under `data_dir`, reading it if it exists. A
    /// file that is not valid JSON is left alone and treated as empty, so
    /// a bad edit never loses the file; a value the app would refuse reads
    /// as its default.
    pub(super) fn open(data_dir: &Path) -> Self {
        let path = data_dir.join(FILE);
        let (file, seen) = read(&path);
        Store {
            path,
            inner: Mutex::new(Inner { file, seen }),
        }
    }

    /// Every setting: the defaults with the file's values over them.
    pub(super) fn get(&self) -> Value {
        let inner = self.inner.lock().unwrap();
        let mut out = defaults();
        merge(&mut out, &inner.file);
        out
    }

    /// One setting by JSON pointer, as it currently is.
    pub(super) fn value(&self, pointer: &str) -> Value {
        self.get().pointer(pointer).cloned().unwrap_or(Value::Null)
    }

    /// Applies a partial object: every leaf checked, unknown keys refused,
    /// the file written atomically. Returns the settings after the change
    /// and the pointers that changed.
    pub(super) fn patch(&self, patch: &Value) -> Result<(Value, Vec<String>), Error> {
        let Value::Object(_) = patch else {
            return Err(Error::invalid("", "must be a JSON object"));
        };
        let mut violations = Vec::new();
        validate(patch, "", &mut violations);
        if !violations.is_empty() {
            violations.sort_by(|a, b| a.path.cmp(&b.path));
            return Err(Error::Invalid(violations));
        }
        let mut inner = self.inner.lock().unwrap();
        let before = {
            let mut v = defaults();
            merge(&mut v, &inner.file);
            v
        };
        let mut file = inner.file.clone();
        merge(&mut file, patch);
        write(&self.path, &file)?;
        inner.file = file;
        inner.seen = stat(&self.path);
        let mut after = defaults();
        merge(&mut after, &inner.file);
        Ok((after.clone(), changed(&before, &after)))
    }

    /// Reads the file again when something else wrote it; the pointers
    /// that changed, or `None` when nothing did.
    pub(super) fn reload_if_changed(&self) -> Option<Vec<String>> {
        let mut inner = self.inner.lock().unwrap();
        let now = stat(&self.path);
        if now == inner.seen {
            return None;
        }
        let (file, seen) = read(&self.path);
        let mut before = defaults();
        merge(&mut before, &inner.file);
        let mut after = defaults();
        merge(&mut after, &file);
        inner.file = file;
        inner.seen = seen;
        let keys = changed(&before, &after);
        (!keys.is_empty()).then_some(keys)
    }
}

fn read(path: &Path) -> (Value, Option<(SystemTime, u64)>) {
    let seen = stat(path);
    let mut file = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(Map::new()));
    drop_refused(&mut file, path);
    (file, seen)
}

/// A hand edit is held to what a change through the API is: a value the
/// app would refuse is left out, so it reads as its default. The file
/// itself is not touched. Unknown keys stay, as they do on a write.
fn drop_refused(file: &mut Value, path: &Path) {
    let mut violations = Vec::new();
    validate(file, "", &mut violations);
    for v in violations.iter().filter(|v| v.message != "unknown setting") {
        let (parent, key) = v.path.rsplit_once('/').unwrap_or(("", &v.path));
        if let Some(Value::Object(map)) = file.pointer_mut(parent) {
            map.remove(key);
            eprintln!(
                "pinrail: {}: {} {}; using the default",
                path.display(),
                v.path,
                v.message
            );
        }
    }
}

fn stat(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Temp file, then rename: the file is whole or untouched.
fn write(path: &Path, value: &Value) -> Result<(), Error> {
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(value).map_err(|e| Error::Internal(e.to_string()))?;
    std::fs::write(&tmp, text + "\n")?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Objects merge key by key; anything else replaces.
fn merge(into: &mut Value, from: &Value) {
    match (into, from) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                match a.get_mut(k) {
                    Some(slot) if slot.is_object() && v.is_object() => merge(slot, v),
                    _ => {
                        a.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (into, from) => *into = from.clone(),
    }
}

fn set_at(root: &mut Value, pointer: &str, value: Value) {
    let mut cur = root;
    let parts: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        let map = match cur {
            Value::Object(m) => m,
            _ => unreachable!("defaults are objects"),
        };
        if i == parts.len() - 1 {
            map.insert((*part).to_string(), value);
            return;
        }
        cur = map
            .entry((*part).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }
}

/// The leaves whose value differs, as pointers; a plugin's settings leaf
/// by leaf.
fn changed(before: &Value, after: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for (path, _, _, _) in LEAVES {
        if before.pointer(path) != after.pointer(path) {
            out.push((*path).to_string());
        }
    }
    let empty = Map::new();
    let plugins = |v: &Value| {
        v.pointer(PLUGINS)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    };
    let (a, b) = (plugins(before), plugins(after));
    for name in a.keys().chain(b.keys()) {
        let (x, y) = (
            a.get(name).and_then(Value::as_object).unwrap_or(&empty),
            b.get(name).and_then(Value::as_object).unwrap_or(&empty),
        );
        for key in x.keys().chain(y.keys()) {
            let pointer = format!("{PLUGINS}/{name}/{key}");
            if x.get(key) != y.get(key) && !out.contains(&pointer) {
                out.push(pointer);
            }
        }
    }
    out
}

/// Walks a patch: known leaves are checked, groups are entered, anything
/// else is refused.
fn validate(value: &Value, pointer: &str, out: &mut Vec<Violation>) {
    if pointer == PLUGINS {
        validate_plugins(value, out);
        return;
    }
    if let Some((_, kind, _, _)) = LEAVES.iter().find(|(p, _, _, _)| *p == pointer) {
        if let Some(message) = check(*kind, value) {
            out.push(Violation::new(pointer, message));
        }
        return;
    }
    let is_group = LEAVES
        .iter()
        .any(|(p, _, _, _)| p.starts_with(&format!("{pointer}/")));
    if !is_group {
        out.push(Violation::new(pointer, "unknown setting"));
        return;
    }
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                validate(v, &format!("{pointer}/{k}"), out);
            }
        }
        _ => out.push(Violation::new(pointer, "must be a JSON object")),
    }
}

/// Plugin settings are one object per plugin, each value a scalar; what
/// the values may be is the plugin's schema, checked by the caller that
/// has the registry.
fn validate_plugins(value: &Value, out: &mut Vec<Violation>) {
    let Value::Object(plugins) = value else {
        out.push(Violation::new(PLUGINS, "must be a JSON object"));
        return;
    };
    for (name, settings) in plugins {
        let pointer = format!("{PLUGINS}/{name}");
        let Value::Object(map) = settings else {
            out.push(Violation::new(pointer, "must be a JSON object"));
            continue;
        };
        for (key, v) in map {
            if !(v.is_boolean() || v.is_number() || v.is_string()) {
                out.push(Violation::new(
                    format!("{pointer}/{key}"),
                    "must be true or false, a number or a string",
                ));
            }
        }
    }
}

fn check(kind: Kind, value: &Value) -> Option<String> {
    let wrong = |what: &str| Some(format!("must be {what}"));
    match kind {
        Kind::Bool => value
            .is_boolean()
            .then_some(())
            .map_or(wrong("true or false"), |_| None),
        Kind::Enum(options) => match value.as_str() {
            Some(s) if options.contains(&s) => None,
            _ => wrong(&format!("one of {}", options.join(", "))),
        },
        Kind::Text => match value.as_str() {
            Some(s) if !s.trim().is_empty() => None,
            _ => wrong("a non-empty string"),
        },
        Kind::NullableTime => match value {
            Value::Null => None,
            Value::String(s) if chrono::DateTime::parse_from_rfc3339(s).is_ok() => None,
            _ => wrong("an RFC 3339 timestamp, or null"),
        },
        Kind::TextList => match value.as_array() {
            Some(items)
                if items
                    .iter()
                    .all(|i| i.as_str().is_some_and(|s| !s.trim().is_empty())) =>
            {
                None
            }
            _ => wrong("a list of names"),
        },
        Kind::NullableHours => match value {
            Value::Null => None,
            Value::Object(map)
                if map.len() == 2
                    && ["from", "to"]
                        .iter()
                        .all(|k| map.get(*k).and_then(Value::as_str).is_some_and(is_clock)) =>
            {
                None
            }
            _ => wrong("{\"from\": \"HH:MM\", \"to\": \"HH:MM\"}, or null"),
        },
        Kind::Port => match value.as_u64() {
            Some(p) if (1..=65535).contains(&p) => None,
            _ => wrong("a port between 1 and 65535"),
        },
        Kind::NullableDays => match value {
            Value::Null => None,
            Value::Number(n) if n.as_u64().is_some_and(|d| d >= 1) => None,
            _ => wrong("a number of days, or null"),
        },
    }
}

fn is_clock(s: &str) -> bool {
    let Some((h, m)) = s.split_once(':') else {
        return false;
    };
    matches!((h.parse::<u8>(), m.parse::<u8>()), (Ok(h), Ok(m)) if h < 24 && m < 60 && h.to_string().len() <= 2 && m.to_string().len() <= 2)
        && h.len() == 2
        && m.len() == 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path());
        (dir, store)
    }

    #[test]
    fn defaults_cover_every_leaf() {
        let d = defaults();
        for (path, _, _, _) in LEAVES {
            assert!(d.pointer(path).is_some(), "{path}");
        }
        assert_eq!(d["port"], 4747);
        assert_eq!(d["appearance"]["theme"], "system");
    }

    #[test]
    fn a_patch_is_merged_written_and_reported() {
        let (dir, store) = store();
        let (after, keys) = store
            .patch(&json!({"appearance": {"theme": "light"}, "notifications": {"sound": false}}))
            .unwrap();
        assert_eq!(after["appearance"]["theme"], "light");
        assert_eq!(
            after["appearance"]["text_size"], "default",
            "the sibling keeps its default"
        );
        assert_eq!(after["notifications"]["sound"], false);
        assert_eq!(keys, vec!["/appearance/theme", "/notifications/sound"]);
        let on_disk: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join(FILE)).unwrap()).unwrap();
        assert_eq!(
            on_disk,
            json!({"appearance": {"theme": "light"}, "notifications": {"sound": false}}),
            "only what was set is written"
        );
        assert!(!dir.path().join("settings.json.tmp").exists());
    }

    #[test]
    fn bad_values_and_unknown_keys_are_refused_with_paths() {
        let (_dir, store) = store();
        let err = store
            .patch(&json!({"appearance": {"theme": "sepia"}, "port": 70000, "nope": 1, "notifications": {"quiet_hours": {"from": "9:00", "to": "17:00"}}}))
            .unwrap_err();
        let Error::Invalid(v) = err else {
            panic!("{err:?}")
        };
        let paths: Vec<&str> = v.iter().map(|x| x.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "/appearance/theme",
                "/nope",
                "/notifications/quiet_hours",
                "/port"
            ]
        );
        assert_eq!(v[0].message, "must be one of system, dark, light");
        assert_eq!(v[1].message, "unknown setting");
        assert!(store.get()["port"] == 4747, "nothing was applied");
    }

    #[test]
    fn nullable_and_shaped_values() {
        let (_dir, store) = store();
        let ok = json!({"notifications": {"paused_until": "2026-09-14T10:00:00Z", "quiet_hours": {"from": "22:00", "to": "07:30"}, "muted_plugins": ["hello"]}, "history": {"keep_days": 30}});
        store.patch(&ok).unwrap();
        store.patch(&json!({"notifications": {"paused_until": null, "quiet_hours": null}, "history": {"keep_days": null}})).unwrap();
        assert!(store.get()["notifications"]["paused_until"].is_null());
        assert!(
            store
                .patch(&json!({"notifications": {"paused_until": "yesterday"}}))
                .is_err()
        );
        assert!(store.patch(&json!({"history": {"keep_days": 0}})).is_err());
        assert!(
            store
                .patch(&json!({"notifications": {"muted_plugins": [""]}}))
                .is_err()
        );
        assert!(
            store.patch(&json!({"notifications": "off"})).is_err(),
            "a group must be an object"
        );
    }

    #[test]
    fn unknown_keys_in_the_file_survive_a_patch() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(FILE),
            r#"{"future": {"thing": 1}, "port": 5000}"#,
        )
        .unwrap();
        let store = Store::open(dir.path());
        assert_eq!(store.get()["port"], 5000);
        assert_eq!(store.get()["future"]["thing"], 1);
        store.patch(&json!({"autostart": true})).unwrap();
        let on_disk: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join(FILE)).unwrap()).unwrap();
        assert_eq!(on_disk["future"]["thing"], 1);
        assert_eq!(on_disk["port"], 5000);
        assert_eq!(on_disk["autostart"], true);
    }

    #[test]
    fn an_outside_edit_is_noticed_with_its_keys() {
        let (dir, store) = store();
        store.patch(&json!({"autostart": true})).unwrap();
        assert!(
            store.reload_if_changed().is_none(),
            "our own write is not a change"
        );
        // a different mtime is not guaranteed within the same second; the
        // length changes here, which is enough
        std::fs::write(
            dir.path().join(FILE),
            r#"{"autostart": false, "appearance": {"theme": "dark"}}"#,
        )
        .unwrap();
        let keys = store.reload_if_changed().unwrap();
        assert_eq!(keys, vec!["/appearance/theme", "/autostart"]);
        assert_eq!(store.get()["appearance"]["theme"], "dark");
        assert!(store.reload_if_changed().is_none());
    }

    #[test]
    fn a_broken_file_is_treated_as_empty_and_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "{ not json").unwrap();
        let store = Store::open(dir.path());
        assert_eq!(store.get()["port"], 4747);
        assert_eq!(
            std::fs::read_to_string(dir.path().join(FILE)).unwrap(),
            "{ not json"
        );
    }

    #[test]
    fn the_port_is_readable_before_anything_opens() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(port_in(dir.path()), None);
        std::fs::write(dir.path().join(FILE), r#"{"port": 4800}"#).unwrap();
        assert_eq!(port_in(dir.path()), Some(4800));
        std::fs::write(dir.path().join(FILE), r#"{"port": 0}"#).unwrap();
        assert_eq!(port_in(dir.path()), None);
    }

    #[test]
    fn a_hand_edited_value_the_app_would_refuse_reads_as_its_default() {
        // A person edits the file by hand; the sweep deletes the history it
        // is told to at start, so 0 ("keep for ever"?) must not reach it
        let dir = tempfile::tempdir().unwrap();
        let text = r#"{"history": {"keep_days": 0}, "port": "4800", "notifications": false, "appearance": {"theme": "light"}}"#;
        std::fs::write(dir.path().join(FILE), text).unwrap();
        let store = Store::open(dir.path());
        let d = defaults();
        assert_eq!(store.value("/history/keep_days"), d["history"]["keep_days"]);
        assert_eq!(store.value("/port"), d["port"]);
        assert_eq!(
            store.value("/notifications/sound"),
            d["notifications"]["sound"]
        );
        assert_eq!(
            store.value("/appearance/theme"),
            "light",
            "a good value next to them stays"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join(FILE)).unwrap(),
            text,
            "the file is left as the person wrote it"
        );

        // the same when the file changes under a running app
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.path().join(FILE), r#"{"history": {"keep_days": -3}}"#).unwrap();
        store.reload_if_changed();
        assert_eq!(store.value("/history/keep_days"), d["history"]["keep_days"]);
    }
}
