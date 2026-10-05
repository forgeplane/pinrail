//! A plugin's recorded decisions checked against what it says it takes and
//! what the app makes of them. Each `fixtures/<name>.decided.json` holds a
//! review's `title`, `payload` and `decision`, and optionally its `origin`
//! and `agent_note`. Its payload and decision must pass the plugin's
//! schemas. When `<name>.decided.md` is beside it, the review rendered as
//! the app renders it must equal that file; when
//! `<name>.decided.summary.json` is, the summary the app sums up from the
//! payload and the decision must equal that one.

use std::path::Path;

use chrono::FixedOffset;
use serde_json::{Value, json};

use crate::markdown::{Head, render_in};
use crate::summary::Rules;

/// Checks every recorded decision in `dir/fixtures`. With `update`, the
/// Markdown, and the summary of a plugin that declares one, are written
/// from what the app makes first, created where they were not there.
///
/// Returns `{"checked": n, "problems": [{"file", "message"}], "written":
/// [file]}`, with paths relative to `dir`. A plugin that does not load
/// checks nothing: the plugin's own check says why.
pub fn check(dir: &Path, update: bool) -> Value {
    let mut checked = 0;
    let mut problems = Vec::new();
    let mut written = Vec::new();
    let plugin = crate::Plugin::load(dir);
    let (Some(payload_schema), Some(decision_schema), None) = (
        plugin.payload_schema.as_ref(),
        plugin.decision_schema.as_ref(),
        plugin.error.as_ref(),
    ) else {
        return json!({ "checked": 0, "problems": [], "written": [] });
    };
    let declares_summary = plugin.summary.request.is_some() || plugin.summary.outcome.is_some();
    let mut files: Vec<_> = std::fs::read_dir(dir.join("fixtures"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".decided.json"))
        })
        .collect();
    files.sort();
    let relative = |p: &Path| {
        p.strip_prefix(dir)
            .unwrap_or(p)
            .to_string_lossy()
            .to_string()
    };
    let beside = |path: &Path, suffix: &str| {
        path.with_file_name(
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .replace(".decided.json", suffix),
        )
    };
    for path in files {
        checked += 1;
        let file = relative(&path);
        let mut problem = |message: String| {
            problems.push(json!({ "file": file, "message": message }));
        };
        let fixture: Value = match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        {
            Ok(v) => v,
            Err(e) => {
                problem(format!("not readable as JSON: {e}"));
                continue;
            }
        };
        let mut valid = true;
        for (what, schema, value) in [
            ("payload", payload_schema, &fixture["payload"]),
            ("decision", decision_schema, &fixture["decision"]["data"]),
        ] {
            if let Some(first) = schema.validate(value).first() {
                valid = false;
                let at = match first.path.trim_start_matches('/') {
                    "" => String::new(),
                    path => format!("{path}: "),
                };
                problem(format!(
                    "the {what} is not what the plugin's schema accepts: {at}{}",
                    first.message
                ));
            }
        }
        if !valid {
            continue;
        }

        // the Markdown an agent reads
        let review = json!({
            "id": "r_fixture", "plugin": plugin.manifest.get("name"), "plugin_version": 1,
            "title": fixture["title"],
            "origin": fixture.get("origin").cloned().unwrap_or(json!({"repo": "acme/api", "workflow": "review", "ref": "42"})),
            "created_at": "2026-09-10T08:00:00Z", "status": "decided",
            "payload": fixture["payload"], "decision": fixture["decision"],
            "agent_note": fixture.get("agent_note").cloned().unwrap_or(Value::Null),
        });
        let rendered = render_in(
            &review,
            None,
            plugin.decision_template.as_deref(),
            FixedOffset::east_opt(0),
            Head::Document,
        );
        let md_path = beside(&path, ".decided.md");
        let md = relative(&md_path);
        if update {
            match std::fs::write(&md_path, &rendered) {
                Ok(()) => written.push(md.clone()),
                Err(e) => problem(format!("{md} could not be written: {e}")),
            }
        }
        // a recorded decision without its Markdown is only shown in tests
        if let Ok(expected) = std::fs::read_to_string(&md_path)
            && expected != rendered
        {
            let line = expected
                .lines()
                .zip(rendered.lines())
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| expected.lines().count().min(rendered.lines().count()))
                + 1;
            problem(format!(
                "{md} does not match what the app renders, from line {line}; check with --update-fixtures to rewrite it, and review the difference"
            ));
        }

        // the summary the inbox and the history show
        let derive = |rules: &Option<Rules>, data: &Value| {
            rules
                .as_ref()
                .and_then(|r| r.derive(data))
                .unwrap_or(Value::Null)
        };
        let summed = json!({
            "request": derive(&plugin.summary.request, &fixture["payload"]),
            "outcome": derive(&plugin.summary.outcome, &fixture["decision"]["data"]),
        });
        let summary_path = beside(&path, ".decided.summary.json");
        let summary = relative(&summary_path);
        if update && (declares_summary || summary_path.is_file()) {
            let text = serde_json::to_string_pretty(&summed).unwrap_or_default() + "\n";
            match std::fs::write(&summary_path, text) {
                Ok(()) => written.push(summary.clone()),
                Err(e) => problem(format!("{summary} could not be written: {e}")),
            }
        }
        if let Ok(text) = std::fs::read_to_string(&summary_path) {
            match serde_json::from_str::<Value>(&text) {
                Ok(expected) if expected == summed => {}
                Ok(_) => problem(format!(
                    "{summary} does not match what the app sums up; check with --update-fixtures to rewrite it, and review the difference"
                )),
                Err(e) => problem(format!("{summary} is not readable as JSON: {e}")),
            }
        }
    }
    json!({ "checked": checked, "problems": problems, "written": written })
}
