//! `pinrail-plugin check` says of a folder what the core's loader says: the
//! rules live in Rust and are carried in JavaScript for authors without the
//! app, so both run here over the sample plugins and a set of broken
//! folders, and their verdicts are compared. A rule changed on one side
//! fails this test until the other follows.

use std::path::{Path, PathBuf};
use std::process::Command;

use pinrail_core::plugins::Plugin;
use serde_json::{Value, json};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// The JavaScript verdict, or None when node is not on the path.
fn js_check(dir: &Path) -> Option<Value> {
    let bin = repo()
        .join("pinrail-plugin")
        .join("bin")
        .join("pinrail-plugin.mjs");
    let out = Command::new("node")
        .arg(&bin)
        .arg("check")
        .arg("--json")
        .arg(dir)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    Some(serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!(
            "check --json on {}: {e}\n{text}\n{}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr)
        )
    }))
}

fn keys(list: &Value) -> Vec<&str> {
    list.as_array()
        .map(|items| items.iter().filter_map(|i| i["key"].as_str()).collect())
        .unwrap_or_default()
}

/// Both sides over one folder: usable as the core sees it, and each feature
/// the core drops warned about by the script, and nothing else.
fn agree(dir: &Path, js: &Value) {
    let plugin = Plugin::load(dir);
    let usable = plugin.error.is_none();
    assert_eq!(
        js["usable"].as_bool(),
        Some(usable),
        "{}: core says {:?}, check says {}",
        dir.display(),
        plugin.error,
        js["problems"]
    );
    if !usable {
        return;
    }
    let warned = keys(&js["warnings"]);
    for (key, dropped) in [
        ("settings_schema", plugin.settings_error.is_some()),
        ("shortcuts", plugin.shortcuts_error.is_some()),
        ("decision_template", plugin.template_error.is_some()),
        ("example", plugin.example_error.is_some()),
        ("sample", plugin.sample_error.is_some()),
        ("icon", plugin.icon_error.is_some()),
    ] {
        assert_eq!(
            warned.contains(&key),
            dropped,
            "{}: {key}: core says {:?}, check warns {:?}",
            dir.display(),
            (
                &plugin.settings_error,
                &plugin.shortcuts_error,
                &plugin.template_error
            ),
            warned
        );
    }
}

/// Files beside the manifest: relative path and content.
type Files = &'static [(&'static str, &'static str)];

fn folder(root: &Path, name: &str, manifest: Value, files: Files) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
    for (file, text) in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

#[test]
fn the_script_and_the_loader_give_the_same_verdicts() {
    let samples = repo().join("plugins");
    let first = samples.join("hello");
    // without node the parity goes unchecked: fail, unless told that is fine
    let Some(js) = js_check(&first) else {
        assert!(
            std::env::var_os("PINRAIL_SKIP_NODE").is_some(),
            "node is not on the path; install it, or set PINRAIL_SKIP_NODE=1 to skip this test"
        );
        return;
    };
    agree(&first, &js);
    for name in ["email", "review", "artifact"] {
        let dir = samples.join(name);
        agree(&dir, &js_check(&dir).unwrap());
    }

    let tmp = tempfile::tempdir().unwrap();
    let base = |extra: Value| {
        let mut m = json!({"name": "sample", "version": "1.0.0", "payload_schema": {}, "decision_schema": {}});
        for (k, v) in extra.as_object().unwrap() {
            if v.is_null() {
                m.as_object_mut().unwrap().remove(k);
            } else {
                m[k] = v.clone();
            }
        }
        m
    };
    let entry: Files = &[("index.html", "<html></html>")];
    let cases: Vec<(&str, Value, Files)> = vec![
        (
            "fine",
            base(json!({"title": "Sample", "icon": "icon.svg", "min_height": 300})),
            &[
                ("index.html", "<html></html>"),
                (
                    "icon.svg",
                    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M4 4h16"/></svg>"#,
                ),
            ],
        ),
        ("icon_missing", base(json!({"icon": "missing.svg"})), entry),
        (
            "icon_not_svg",
            base(json!({"icon": "icon.svg"})),
            &[("index.html", ""), ("icon.svg", "hello")],
        ),
        ("young", base(json!({"version": "0.1.0"})), entry),
        ("zero", base(json!({"version": "0.0.0"})), entry),
        ("integer", base(json!({"version": 2})), entry),
        ("no_version", base(json!({"version": null})), entry),
        ("bad_name", base(json!({"name": "Bad-Name"})), entry),
        ("no_entry", base(json!({"entry": "view/index.html"})), entry),
        (
            "builds_later",
            base(json!({"entry": "view/index.html", "build": {"command": "npm run build"}})),
            &[],
        ),
        ("abs_entry", base(json!({"entry": "/index.html"})), entry),
        ("no_schema", base(json!({"decision_schema": null})), entry),
        (
            "ref_out",
            base(json!({"payload_schema": {"$ref": "../x.json"}})),
            entry,
        ),
        (
            "ref_missing",
            base(json!({"payload_schema": {"$ref": "schemas/p.json"}})),
            entry,
        ),
        (
            "ref_ok",
            base(json!({"payload_schema": {"$ref": "schemas/p.json"}})),
            &[
                ("index.html", ""),
                ("schemas/p.json", r#"{"type":"object"}"#),
            ],
        ),
        ("bad_icon", base(json!({"icon": "Mail"})), entry),
        // the manifest schema's rules, which both sides read from one file
        ("title_number", base(json!({"title": 3})), entry),
        (
            "description_list",
            base(json!({"description": ["a"]})),
            entry,
        ),
        ("icon_dash", base(json!({"icon": "-mail"})), entry),
        ("min_height_zero", base(json!({"min_height": 0})), entry),
        ("min_height_text", base(json!({"min_height": "400"})), entry),
        ("dev_text", base(json!({"dev": "yes"})), entry),
        ("empty_entry", base(json!({"entry": ""})), entry),
        ("build_text", base(json!({"build": "npm run build"})), entry),
        ("build_empty", base(json!({"build": {}})), entry),
        (
            "build_blank",
            base(json!({"build": {"command": "  "}})),
            entry,
        ),
        (
            "schema_text",
            base(json!({"payload_schema": "p.json"})),
            entry,
        ),
        ("version_short", base(json!({"version": "1.2"})), entry),
        (
            "extra_key",
            base(
                json!({"$schema": "https://pinrail.dev/schemas/manifest.schema.json", "later": 1}),
            ),
            entry,
        ),
        (
            "settings_text",
            base(json!({"settings_schema": "s.json"})),
            entry,
        ),
        ("shortcuts_text", base(json!({"shortcuts": "j"})), entry),
        (
            "shortcut_empty_keys",
            base(json!({"shortcuts": [{"keys": "", "does": "x"}]})),
            entry,
        ),
        (
            "template_outside",
            base(json!({"decision_template": "../t.j2"})),
            entry,
        ),
        (
            "template_number",
            base(json!({"decision_template": 1})),
            entry,
        ),
        // an example payload, which must pass the payload schema
        (
            "example_ok",
            base(
                json!({"payload_schema": {"type": "object", "required": ["n"]}, "example": "ex.json"}),
            ),
            &[("index.html", ""), ("ex.json", r#"{"n": 1}"#)],
        ),
        (
            "example_fails",
            base(
                json!({"payload_schema": {"type": "object", "required": ["n"]}, "example": "ex.json"}),
            ),
            &[("index.html", ""), ("ex.json", r#"{"m": 1}"#)],
        ),
        (
            "example_missing",
            base(json!({"example": "ex.json"})),
            entry,
        ),
        (
            "example_outside",
            base(json!({"example": "../ex.json"})),
            entry,
        ),
        // a sample: a request with a title, a payload that passes, its files
        (
            "sample_ok",
            base(
                json!({"payload_schema": {"type": "object", "required": ["n"]}, "sample": "s.json"}),
            ),
            &[
                ("index.html", ""),
                (
                    "s.json",
                    r#"{"title": "t", "payload": {"n": 1}, "attachments": {"a.png": "a.png"}}"#,
                ),
                ("a.png", "png"),
            ],
        ),
        (
            "sample_untitled",
            base(json!({"sample": "s.json"})),
            &[("index.html", ""), ("s.json", r#"{"payload": {}}"#)],
        ),
        (
            "sample_fails",
            base(
                json!({"payload_schema": {"type": "object", "required": ["n"]}, "sample": "s.json"}),
            ),
            &[
                ("index.html", ""),
                ("s.json", r#"{"title": "t", "payload": {"m": 1}}"#),
            ],
        ),
        (
            "sample_file_missing",
            base(json!({"sample": "s.json"})),
            &[
                ("index.html", ""),
                (
                    "s.json",
                    r#"{"title": "t", "payload": {}, "attachments": {"a.png": "a.png"}}"#,
                ),
            ],
        ),
        (
            "sample_outside",
            base(json!({"sample": "../s.json"})),
            entry,
        ),
        (
            "use_when",
            base(json!({"use_when": "Before posting review comments"})),
            entry,
        ),
        (
            "settings_ok",
            base(
                json!({"settings_schema": {"type": "object", "properties": {"wrap": {"type": "boolean", "default": true}}}}),
            ),
            entry,
        ),
        (
            "settings_no_default",
            base(json!({"settings_schema": {"properties": {"wrap": {"type": "boolean"}}}})),
            entry,
        ),
        (
            "settings_object",
            base(
                json!({"settings_schema": {"properties": {"a": {"type": "object", "default": {}}}}}),
            ),
            entry,
        ),
        (
            "settings_enum",
            base(
                json!({"settings_schema": {"properties": {"a": {"type": "string", "default": "x", "enum": [1]}}}}),
            ),
            entry,
        ),
        (
            "shortcuts_ok",
            base(json!({"shortcuts": [{"keys": "Cmd+Shift+F", "does": "Fold", "group": "View"}]})),
            entry,
        ),
        (
            "shortcuts_modifier",
            base(json!({"shortcuts": [{"keys": "hyper+j", "does": "x"}]})),
            entry,
        ),
        (
            "shortcuts_no_does",
            base(json!({"shortcuts": [{"keys": "j"}]})),
            entry,
        ),
        (
            "shortcuts_shape",
            base(json!({"shortcuts": {"keys": "j"}})),
            entry,
        ),
        (
            "template_ok",
            base(json!({"decision_template": "t.j2"})),
            &[("index.html", ""), ("t.j2", "{{ note }}")],
        ),
        (
            "template_missing",
            base(json!({"decision_template": "t.j2"})),
            entry,
        ),
        (
            "template_out",
            base(json!({"decision_template": "../t.j2"})),
            entry,
        ),
    ];
    for (name, manifest, files) in cases {
        let dir = folder(tmp.path(), name, manifest, files);
        agree(&dir, &js_check(&dir).unwrap());
    }
}
