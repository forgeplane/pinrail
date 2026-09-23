//! `pinrail plugins describe`: what an agent needs to ask with Pinrail, from
//! one command. The server describes the plugins; the CLI adds how to
//! submit, where the decision lands and what each exit code means.

use serde_json::{Value, json};

use crate::{EXIT_CLOSED, EXIT_DISCARDED, EXIT_ERROR, EXIT_REFUSED, EXIT_TIMEOUT};

const SUBMIT: &str =
    "pinrail submit <plugin> --title \"<what it is about>\" --data payload.json --wait";
const CHECK: &str =
    "pinrail submit <plugin> --title \"<what it is about>\" --data payload.json --dry-run";
const FILES: &str = "for a plugin with `artifacts`, send each file its payload schema asks for with --artifact PATH[=NAME]; the schema says where a file goes, as {\"$artifact\": \"<name>\"}, and --dry-run checks it all before anything is uploaded";

/// What each exit code tells the agent to do next.
const EXIT_CODES: &[(u8, &str)] = &[
    (0, "decided: read decision.data and act on it"),
    (EXIT_ERROR, "error: bad arguments, server unreachable, I/O"),
    (
        EXIT_REFUSED,
        "refused: the payload or the arguments failed a check; the violations are on stderr, each with a JSON pointer",
    ),
    (EXIT_CLOSED, "withdrawn or expired instead of decided"),
    (
        EXIT_TIMEOUT,
        "timed out: the review is still pending; wait again with `pinrail wait <id>`",
    ),
    (
        EXIT_DISCARDED,
        "discarded: the person said no; stop the work the review was gating",
    ),
];

const RESULT: &[(&str, &str)] = &[
    ("status", "decided, withdrawn, expired or discarded"),
    (
        "decision.data",
        "the person's answer, shaped by the plugin's decision_schema",
    ),
    (
        "agent_note",
        "a note from the person to the agent, when they left one",
    ),
];

/// The server's description with the CLI's own part added.
pub fn document(described: Value) -> Value {
    let exit_codes: serde_json::Map<String, Value> = EXIT_CODES
        .iter()
        .map(|(code, meaning)| (code.to_string(), json!(meaning)))
        .collect();
    let result: serde_json::Map<String, Value> = RESULT
        .iter()
        .map(|(key, meaning)| (key.to_string(), json!(meaning)))
        .collect();
    json!({
        "plugins": described["plugins"],
        "submit": {
            "command": SUBMIT,
            "check": CHECK,
            "result": result,
            "markdown": "--format markdown prints a decided review as markdown, for a plugin whose `markdown` is true",
            "artifacts": FILES,
            "exit_codes": exit_codes,
        },
    })
}

pub fn markdown(described: &Value) -> String {
    let mut out = String::from("# Pinrail plugins\n\n");
    out.push_str(
        "Pinrail puts a question to a person and hands their decision back. Each plugin is one kind of question: pick the one whose *Use when* fits, send a payload its schema accepts, and read the decision.\n",
    );
    let plugins = described["plugins"].as_array().cloned().unwrap_or_default();
    if plugins.is_empty() {
        out.push_str("\nNo plugin is installed and usable.\n");
    }
    for plugin in &plugins {
        plugin_section(&mut out, plugin);
    }

    out.push_str("\n## Submitting\n\n");
    out.push_str(&format!("```sh\n{SUBMIT}\n```\n\n"));
    out.push_str(&format!(
        "Check a payload first, without creating a review:\n\n```sh\n{CHECK}\n```\n\n"
    ));
    out.push_str(&format!(
        "Files go beside the payload for a plugin that takes them ({}).\n\n",
        FILES.trim_start_matches("for a plugin with `artifacts`, ")
    ));
    out.push_str("The review printed once it is decided carries:\n\n");
    for (key, meaning) in RESULT {
        out.push_str(&format!("- `{key}`: {meaning}\n"));
    }
    out.push_str("\n## Exit codes\n\n| code | meaning |\n|---|---|\n");
    for (code, meaning) in EXIT_CODES {
        out.push_str(&format!("| {code} | {meaning} |\n"));
    }
    out
}

fn plugin_section(out: &mut String, plugin: &Value) {
    let text = |key: &str| plugin[key].as_str().filter(|s| !s.is_empty());
    let name = text("name").unwrap_or_default();
    out.push_str(&format!(
        "\n## {} (`{name}`) · {}\n\n",
        text("title").unwrap_or(name),
        text("release").unwrap_or_default()
    ));
    if let Some(description) = text("description") {
        out.push_str(&format!("{description}\n\n"));
    }
    if let Some(use_when) = text("use_when") {
        out.push_str(&format!("**Use when:** {use_when}\n\n"));
    }
    out.push_str(&format!(
        "### Payload\n\n{}",
        json_block(&plugin["payload_schema"])
    ));
    if !plugin["example"].is_null() {
        out.push_str(&format!("\nExample:\n\n{}", json_block(&plugin["example"])));
    }
    if let Some(files) = plugin["artifacts"].as_object() {
        let kinds: Vec<&str> = files
            .get("accept")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let mut limits = Vec::new();
        if let Some(size) = files.get("max_size").and_then(Value::as_u64) {
            limits.push(format!("up to {} MB each", size / (1024 * 1024)));
        }
        if let Some(count) = files.get("max_count").and_then(Value::as_u64) {
            limits.push(format!("{count} at most"));
        }
        out.push_str(&format!(
            "\n### Files\n\nTakes files beside the payload: {}{}. The payload schema above says where each goes, as `{{\"$artifact\": \"<name>\"}}`; send each file it names with `--artifact PATH[=NAME]`.\n\n```sh\npinrail submit {name} --title \"<what it is about>\" --data payload.json --artifact <file> --wait\n```\n",
            kinds.join(", "),
            if limits.is_empty() { String::new() } else { format!(" ({})", limits.join(", ")) },
        ));
    }
    out.push_str(&format!(
        "\n### Decision\n\n`decision.data` is shaped by:\n\n{}",
        json_block(&plugin["decision_schema"])
    ));
}

fn json_block(value: &Value) -> String {
    format!(
        "```json\n{}\n```\n",
        serde_json::to_string_pretty(value).unwrap_or_default()
    )
}
