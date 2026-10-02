//! What the format makes of a folder: which plugins it takes, and which
//! feature a plugin loses for each rule its manifest breaks. Each case is a
//! folder and the verdict expected of it.

use std::path::{Path, PathBuf};

use pinrail_format::Plugin;
use serde_json::{Value, json};

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

/// The verdict in brief: whether the plugin is taken, and the keys of the
/// features it would lose, sorted.
fn brief(dir: &Path) -> (bool, Vec<String>) {
    let verdict = Plugin::check(dir);
    let mut keys: Vec<String> = verdict["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| w["key"].as_str().map(str::to_string))
        .collect();
    keys.sort();
    (verdict["usable"] == true, keys)
}

#[test]
fn each_folder_gets_the_verdict_its_rules_give() {
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
        (
            "entry_outside",
            base(json!({"entry": "../entry_outside/index.html"})),
            entry,
        ),
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
        // a summary that does not read costs the plugin its summaries
        ("summary_number", base(json!({"summary": 3})), entry),
        (
            "summary_side",
            base(json!({"summary": {"reqest": {}}})),
            entry,
        ),
        (
            "summary_tone",
            base(
                json!({"summary": {"request": {"counts": [{"items": "/a", "label": "a", "tone": "red"}]}}}),
            ),
            entry,
        ),
        (
            "summary_pointer",
            base(
                json!({"summary": {"outcome": {"counts": [{"items": "decisions", "label": "a"}]}}}),
            ),
            entry,
        ),
        (
            "summary_too_many",
            base(
                json!({"summary": {"outcome": {"counts": [{"items": "/d", "by": "action", "values": {"a": {}, "b": {}, "c": {}, "d": {}, "e": {}, "f": {}}}]}}}),
            ),
            entry,
        ),
        (
            "summary_verdict_star",
            base(json!({"summary": {"outcome": {"verdict": {"at": "/v/*", "values": {"a": {}}}}}})),
            entry,
        ),
        (
            "summary_ok",
            base(json!({"summary": {
                "request": {"counts": [{"items": "/groups/*/items", "by": "severity", "values": {"high": {"tone": "danger"}}, "other": false}]},
                "outcome": {"counts": [{"items": "/undecided", "label": "undecided"}], "verdict": {"at": "/verdict", "values": {"approve": {"label": "approved", "tone": "success"}}}}
            }})),
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
    let expected: &[(&str, bool, &[&str])] = &[
        ("fine", true, &[]),
        ("icon_missing", true, &["icon"]),
        ("icon_not_svg", true, &["icon"]),
        ("young", true, &[]),
        ("zero", false, &[]),
        ("integer", false, &[]),
        ("no_version", false, &[]),
        ("bad_name", false, &[]),
        ("no_entry", false, &[]),
        ("builds_later", true, &["entry"]),
        ("abs_entry", false, &[]),
        ("entry_outside", false, &[]),
        ("no_schema", false, &[]),
        ("ref_out", false, &[]),
        ("ref_missing", false, &[]),
        ("ref_ok", true, &[]),
        ("bad_icon", true, &["icon"]),
        ("title_number", false, &[]),
        ("description_list", false, &[]),
        ("icon_dash", true, &["icon"]),
        ("min_height_zero", false, &[]),
        ("min_height_text", false, &[]),
        ("dev_text", false, &[]),
        ("empty_entry", false, &[]),
        ("build_text", false, &[]),
        ("build_empty", false, &[]),
        ("build_blank", false, &[]),
        ("schema_text", false, &[]),
        ("version_short", false, &[]),
        ("extra_key", true, &["later"]),
        ("settings_text", true, &["settings_schema"]),
        ("shortcuts_text", true, &["shortcuts"]),
        ("shortcut_empty_keys", true, &["shortcuts"]),
        ("template_outside", true, &["decision_template"]),
        ("template_number", true, &["decision_template"]),
        ("summary_number", true, &["summary"]),
        ("summary_side", true, &["summary"]),
        ("summary_tone", true, &["summary"]),
        ("summary_pointer", true, &["summary"]),
        ("summary_too_many", true, &["summary"]),
        ("summary_verdict_star", true, &["summary"]),
        ("summary_ok", true, &[]),
        ("example_ok", true, &[]),
        ("example_fails", true, &["example"]),
        ("example_missing", true, &["example"]),
        ("example_outside", true, &["example"]),
        ("sample_ok", true, &[]),
        ("sample_untitled", true, &["sample"]),
        ("sample_fails", true, &["sample"]),
        ("sample_file_missing", true, &["sample"]),
        ("sample_outside", true, &["sample"]),
        ("use_when", true, &[]),
        ("settings_ok", true, &[]),
        ("settings_no_default", true, &["settings_schema"]),
        ("settings_object", true, &["settings_schema"]),
        ("settings_enum", true, &["settings_schema"]),
        ("shortcuts_ok", true, &[]),
        ("shortcuts_modifier", true, &["shortcuts"]),
        ("shortcuts_no_does", true, &["shortcuts"]),
        ("shortcuts_shape", true, &["shortcuts"]),
        ("template_ok", true, &[]),
        ("template_missing", true, &["decision_template"]),
        ("template_out", true, &["decision_template"]),
    ];
    assert_eq!(cases.len(), expected.len(), "a case without its verdict");
    for ((name, manifest, files), (expected_name, usable, keys)) in cases.into_iter().zip(expected)
    {
        assert_eq!(
            name, *expected_name,
            "the cases and the verdicts are in one order"
        );
        let dir = folder(tmp.path(), name, manifest, files);
        let keys: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
        assert_eq!(
            brief(&dir),
            (*usable, keys),
            "{name}: {}",
            Plugin::check(&dir)
        );
    }
}

/// Every official plugin is taken whole, with nothing dropped.
#[test]
fn every_official_plugin_is_taken_with_nothing_dropped() {
    let plugins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let mut seen = 0;
    for dir in std::fs::read_dir(&plugins)
        .unwrap()
        .flatten()
        .map(|e| e.path())
    {
        if !dir.join("manifest.json").is_file() {
            continue;
        }
        assert_eq!(
            brief(&dir),
            (true, Vec::new()),
            "{}: {}",
            dir.display(),
            Plugin::check(&dir)
        );
        seen += 1;
    }
    assert!(seen >= 9, "only {seen} plugins found");
}
