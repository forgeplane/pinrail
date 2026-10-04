//! What the format makes of a folder: which plugins it takes, and which
//! feature a plugin loses for each rule its manifest breaks. Each case is a
//! folder and the verdict expected of it.

use std::path::{Path, PathBuf};

use pinrail_format::Plugin;
use serde_json::{Value, json};

/// Files beside the manifest: relative path and content. A file named with
/// no content, `("view/index.html", DELETE)`, is taken away from the files
/// every plugin starts with.
type Files = &'static [(&'static str, &'static str)];

const DELETE: &str = "\u{0}delete";

/// What every plugin starts with besides its manifest: a view and two
/// schemas that take anything.
const BASE: Files = &[
    ("view/index.html", "<html></html>"),
    ("schemas/payload.schema.json", "{}"),
    ("schemas/decision.schema.json", "{}"),
];

fn folder(root: &Path, name: &str, manifest: Value, files: Files) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
    for (file, text) in BASE.iter().chain(files.iter()) {
        let path = dir.join(file);
        if *text == DELETE {
            std::fs::remove_file(&path).unwrap();
            continue;
        }
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
        let mut m = json!({"name": "sample", "version": "1.0.0"});
        for (k, v) in extra.as_object().unwrap() {
            if v.is_null() {
                m.as_object_mut().unwrap().remove(k);
            } else {
                m[k] = v.clone();
            }
        }
        m
    };
    let none: Files = &[];
    let cases: Vec<(&str, Value, Files, bool, &[&str])> = vec![
        (
            "fine",
            base(json!({"title": "Sample"})),
            &[(
                "icon.svg",
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M4 4h16"/></svg>"#,
            )],
            true,
            &[],
        ),
        // the files in their fixed places
        (
            "no_view",
            base(json!({})),
            &[("view/index.html", DELETE)],
            false,
            &[],
        ),
        (
            "not_built",
            base(json!({})),
            &[("view/index.html", DELETE)],
            false,
            &[],
        ),
        (
            "no_payload_schema",
            base(json!({})),
            &[("schemas/payload.schema.json", DELETE)],
            false,
            &[],
        ),
        (
            "no_decision_schema",
            base(json!({})),
            &[("schemas/decision.schema.json", DELETE)],
            false,
            &[],
        ),
        (
            "payload_schema_not_json",
            base(json!({})),
            &[("schemas/payload.schema.json", "{")],
            false,
            &[],
        ),
        (
            "schema_ref_ok",
            base(json!({})),
            &[
                ("schemas/payload.schema.json", r#"{"$ref": "defs.json"}"#),
                ("schemas/defs.json", r#"{"type": "object"}"#),
            ],
            true,
            &[],
        ),
        (
            "schema_ref_out",
            base(json!({})),
            &[("schemas/payload.schema.json", r#"{"$ref": "../../x.json"}"#)],
            false,
            &[],
        ),
        (
            "icon_not_svg",
            base(json!({})),
            &[("icon.svg", "hello")],
            true,
            &["icon"],
        ),
        (
            "template_ok",
            base(json!({})),
            &[("templates/decision.md.j2", "{{ note }}")],
            true,
            &[],
        ),
        (
            "template_broken",
            base(json!({})),
            &[("templates/decision.md.j2", "{% if %}")],
            true,
            &["template"],
        ),
        // keys that once named those files: still loaded, and told
        (
            "entry_key",
            base(json!({"entry": "view/index.html"})),
            none,
            true,
            &["entry"],
        ),
        (
            "icon_key",
            base(json!({"icon": "Mail"})),
            none,
            true,
            &["icon"],
        ),
        (
            "schema_keys",
            base(json!({"payload_schema": {}, "decision_schema": {"$ref": "d.json"}})),
            none,
            true,
            &["decision_schema", "payload_schema"],
        ),
        (
            "template_key",
            base(json!({"decision_template": "t.j2"})),
            none,
            true,
            &["decision_template"],
        ),
        // the manifest's own rules
        ("young", base(json!({"version": "0.1.0"})), none, true, &[]),
        ("zero", base(json!({"version": "0.0.0"})), none, false, &[]),
        ("integer", base(json!({"version": 2})), none, false, &[]),
        (
            "no_version",
            base(json!({"version": null})),
            none,
            false,
            &[],
        ),
        (
            "bad_name",
            base(json!({"name": "Bad-Name"})),
            none,
            false,
            &[],
        ),
        ("title_number", base(json!({"title": 3})), none, false, &[]),
        (
            "description_list",
            base(json!({"description": ["a"]})),
            none,
            false,
            &[],
        ),
        ("dev_text", base(json!({"dev": "yes"})), none, false, &[]),
        (
            "version_short",
            base(json!({"version": "1.2"})),
            none,
            false,
            &[],
        ),
        (
            "extra_key",
            base(
                json!({"$schema": "https://pinrail.dev/schemas/manifest.schema.json", "later": 1}),
            ),
            none,
            true,
            &["later"],
        ),
        (
            "use_when",
            base(json!({"use_when": "Before posting review comments"})),
            none,
            true,
            &[],
        ),
        // settings and shortcuts
        (
            "settings_text",
            base(json!({"settings_schema": "s.json"})),
            none,
            true,
            &["settings_schema"],
        ),
        (
            "settings_ok",
            base(
                json!({"settings_schema": {"type": "object", "properties": {"wrap": {"type": "boolean", "default": true}}}}),
            ),
            none,
            true,
            &[],
        ),
        (
            "settings_no_default",
            base(json!({"settings_schema": {"properties": {"wrap": {"type": "boolean"}}}})),
            none,
            true,
            &["settings_schema"],
        ),
        (
            "settings_object",
            base(
                json!({"settings_schema": {"properties": {"a": {"type": "object", "default": {}}}}}),
            ),
            none,
            true,
            &["settings_schema"],
        ),
        (
            "settings_enum",
            base(
                json!({"settings_schema": {"properties": {"a": {"type": "string", "default": "x", "enum": [1]}}}}),
            ),
            none,
            true,
            &["settings_schema"],
        ),
        (
            "shortcuts_text",
            base(json!({"shortcuts": "j"})),
            none,
            true,
            &["shortcuts"],
        ),
        (
            "shortcut_empty_keys",
            base(json!({"shortcuts": [{"keys": "", "does": "x"}]})),
            none,
            true,
            &["shortcuts"],
        ),
        (
            "shortcuts_ok",
            base(json!({"shortcuts": [{"keys": "Cmd+Shift+F", "does": "Fold", "group": "View"}]})),
            none,
            true,
            &[],
        ),
        (
            "shortcuts_modifier",
            base(json!({"shortcuts": [{"keys": "hyper+j", "does": "x"}]})),
            none,
            true,
            &["shortcuts"],
        ),
        (
            "shortcuts_no_does",
            base(json!({"shortcuts": [{"keys": "j"}]})),
            none,
            true,
            &["shortcuts"],
        ),
        (
            "shortcuts_shape",
            base(json!({"shortcuts": {"keys": "j"}})),
            none,
            true,
            &["shortcuts"],
        ),
        // a summary that does not read costs the plugin its summaries
        (
            "summary_number",
            base(json!({"summary": 3})),
            none,
            true,
            &["summary"],
        ),
        (
            "summary_side",
            base(json!({"summary": {"reqest": {}}})),
            none,
            true,
            &["summary"],
        ),
        (
            "summary_tone",
            base(
                json!({"summary": {"request": {"counts": [{"items": "/a", "label": "a", "tone": "red"}]}}}),
            ),
            none,
            true,
            &["summary"],
        ),
        (
            "summary_pointer",
            base(
                json!({"summary": {"outcome": {"counts": [{"items": "decisions", "label": "a"}]}}}),
            ),
            none,
            true,
            &["summary"],
        ),
        (
            "summary_too_many",
            base(
                json!({"summary": {"outcome": {"counts": [{"items": "/d", "by": "action", "values": {"a": {}, "b": {}, "c": {}, "d": {}, "e": {}, "f": {}}}]}}}),
            ),
            none,
            true,
            &["summary"],
        ),
        (
            "summary_verdict_star",
            base(json!({"summary": {"outcome": {"verdict": {"at": "/v/*", "values": {"a": {}}}}}})),
            none,
            true,
            &["summary"],
        ),
        (
            "summary_ok",
            base(json!({"summary": {
                "request": {"counts": [{"items": "/groups/*/items", "by": "severity", "values": {"high": {"tone": "danger"}}, "other": false}]},
                "outcome": {"counts": [{"items": "/undecided", "label": "undecided"}], "verdict": {"at": "/verdict", "values": {"approve": {"label": "approved", "tone": "success"}}}}
            }})),
            none,
            true,
            &[],
        ),
        // samples: requests with a title and a payload that passes, their
        // files beside them in samples/
        (
            "sample_ok",
            base(json!({})),
            &[
                (
                    "schemas/payload.schema.json",
                    r#"{"type": "object", "required": ["n"]}"#,
                ),
                (
                    "samples/s.json",
                    r#"{"title": "t", "payload": {"n": 1}, "attachments": {"a.png": "a.png"}}"#,
                ),
                ("samples/a.png", "png"),
            ],
            true,
            &[],
        ),
        (
            "sample_untitled",
            base(json!({})),
            &[("samples/s.json", r#"{"payload": {}}"#)],
            true,
            &["samples"],
        ),
        (
            "sample_fails",
            base(json!({})),
            &[
                (
                    "schemas/payload.schema.json",
                    r#"{"type": "object", "required": ["n"]}"#,
                ),
                ("samples/s.json", r#"{"title": "t", "payload": {"m": 1}}"#),
            ],
            true,
            &["samples"],
        ),
        (
            "sample_file_missing",
            base(json!({})),
            &[(
                "samples/s.json",
                r#"{"title": "t", "payload": {}, "attachments": {"a.png": "a.png"}}"#,
            )],
            true,
            &["samples"],
        ),
        (
            "sample_file_outside",
            base(json!({})),
            &[(
                "samples/s.json",
                r#"{"title": "t", "payload": {}, "attachments": {"a.png": "../../a.png"}}"#,
            )],
            true,
            &["samples"],
        ),
        (
            "samples_one_bad",
            base(json!({})),
            &[
                ("samples/a.json", r#"{"title": "a", "payload": {}}"#),
                ("samples/b.json", "{"),
            ],
            true,
            &["samples"],
        ),
        // the keys that named the example and the sample
        (
            "example_key",
            base(json!({"example": "ex.json"})),
            none,
            true,
            &["example"],
        ),
        (
            "sample_key",
            base(json!({"sample": "s.json"})),
            none,
            true,
            &["sample"],
        ),
    ];
    for (name, manifest, files, usable, keys) in cases {
        let dir = folder(tmp.path(), name, manifest, files);
        let keys: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
        assert_eq!(
            brief(&dir),
            (usable, keys),
            "{name}: {}",
            Plugin::check(&dir)
        );
    }
}

/// A plugin without samples is taken, with a note that agents and people
/// get nothing to start from; one with samples gets no such note.
#[test]
fn a_plugin_without_samples_is_told_in_a_note() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = json!({"name": "bare", "version": "1.0.0"});
    let bare = folder(tmp.path(), "bare", manifest.clone(), &[]);
    let verdict = Plugin::check(&bare);
    assert_eq!(verdict["usable"], true);
    assert_eq!(verdict["notes"][0]["key"], "samples", "{verdict}");
    let sampled = folder(
        tmp.path(),
        "sampled",
        manifest,
        &[("samples/s.json", r#"{"title": "t", "payload": {}}"#)],
    );
    assert_eq!(Plugin::check(&sampled)["notes"], json!([]));
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
