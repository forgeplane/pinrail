//! `pinrail plugins describe`: what an agent needs to ask with Pinrail. With
//! no name it is an index, each plugin in a line with when to use it, so an
//! agent picks one without reading every schema; with a name, or `--all`,
//! the full description. The server describes the plugins; the CLI adds how
//! to submit, where the decision lands and what each exit code means.

use serde_json::{Value, json};

use crate::{EXIT_CLOSED, EXIT_DISCARDED, EXIT_ERROR, EXIT_REFUSED, EXIT_TIMEOUT};

const SUBMIT: &str = "pinrail submit <plugin> --title \"<what it is about>\" --origin repo=<owner/name>,ref=<branch or PR> --data payload.json --wait";
const CHECK: &str =
    "pinrail submit <plugin> --title \"<what it is about>\" --data payload.json --dry-run";
const ORIGIN: &str = "say where the review comes from with --origin: repo is the project, ref the branch or pull request, url a link back when there is one. Inside a git checkout the command fills repo (owner/name, from the remote) and ref (the branch) itself; outside one, give repo a short descriptive name for the project. The inbox groups reviews by repo, and one without it lands under No project";
const FILES: &str = "for a plugin with `attachments`, send each file its payload schema asks for with --attach PATH[=NAME]; the schema says where a file goes, as {\"$attachment\": \"<name>\"}, and --dry-run checks it all before anything is uploaded";

const NEXT: &str = "run `pinrail plugins describe <name>` for a plugin's payload schema, an example payload and the decision it returns, before you submit to it";

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

/// A plugin in the index: when to use it, and what else sets it apart.
fn summary(plugin: &Value) -> Value {
    json!({
        "name": plugin["name"],
        "title": plugin["title"],
        "use_when": plugin["use_when"].as_str().or(plugin["description"].as_str()),
        "files": plugin["attachments"]["accept"],
        "sample": plugin["sample"].as_bool().unwrap_or(false),
    })
}

/// The server's description with the CLI's own part added: every plugin in
/// full, or, as an `index`, each in a line with the way to its full part.
pub fn document(described: Value, index: bool) -> Value {
    let exit_codes: serde_json::Map<String, Value> = EXIT_CODES
        .iter()
        .map(|(code, meaning)| (code.to_string(), json!(meaning)))
        .collect();
    let result: serde_json::Map<String, Value> = RESULT
        .iter()
        .map(|(key, meaning)| (key.to_string(), json!(meaning)))
        .collect();
    let plugins = if index {
        Value::Array(
            described["plugins"]
                .as_array()
                .map(|all| all.iter().map(summary).collect())
                .unwrap_or_default(),
        )
    } else {
        described["plugins"].clone()
    };
    let mut doc = json!({
        "plugins": plugins,
        "submit": {
            "command": SUBMIT,
            "check": CHECK,
            "origin": ORIGIN,
            "result": result,
            "markdown": "a decided review prints as markdown by default, shaped by the plugin's template when its `markdown` is true; --json prints it as JSON",
            "attachments": FILES,
            "exit_codes": exit_codes,
        },
    });
    if index {
        doc["next"] = json!(NEXT);
        doc["docs"] = json!("pinrail docs: how to ask, and what to do with the answer");
    }
    doc
}

/// As markdown: the index, every plugin in full, or, `named`, the one
/// plugin asked for, which opens the document and fills the commands.
pub fn markdown(described: &Value, index: bool, named: bool) -> String {
    let plugins = described["plugins"].as_array().cloned().unwrap_or_default();
    if named && let [plugin] = plugins.as_slice() {
        let mut section = String::new();
        plugin_section(&mut section, plugin);
        // the plugin is the document: its heading the title, the rest one level up
        let mut out = section
            .trim_start()
            .replacen("## ", "# ", 1)
            .replace("\n### ", "\n## ");
        // how to send it; the rest, exit codes included, is `pinrail docs asking`
        let name = plugin["name"].as_str().unwrap_or("<plugin>");
        out.push_str(&format!(
            "\n## Submitting\n\n```sh\n{}\n```\n\nExit codes, rounds and the rest: `pinrail docs asking`.\n",
            SUBMIT.replace("<plugin>", name)
        ));
        return out;
    }
    let mut out = String::from("# Pinrail plugins\n\n");
    out.push_str(
        "Pinrail puts a question to a person and hands their decision back. Each plugin is one kind of question: pick the one whose *Use when* fits, send a payload its schema accepts, and read the decision.\n",
    );
    if plugins.is_empty() {
        out.push_str("\nNo plugin is installed and usable.\n");
    }
    if index {
        out.push('\n');
        for plugin in &plugins {
            index_line(&mut out, plugin);
        }
        out.push_str(&format!(
            "\nNext, {NEXT}.\n\nHow to ask, and what to do with the answer: `pinrail docs`.\n"
        ));
    } else {
        for plugin in &plugins {
            plugin_section(&mut out, plugin);
        }
    }

    out.push_str(&general("<plugin>"));
    out
}

/// How to submit, what comes back and the exit codes, for `plugin`.
fn general(plugin: &str) -> String {
    let mut out = String::new();
    out.push_str("\n## Submitting\n\n");
    out.push_str(&format!(
        "```sh\n{}\n```\n\n",
        SUBMIT.replace("<plugin>", plugin)
    ));
    out.push_str(&format!(
        "{}{}.\n\n",
        ORIGIN[..1].to_uppercase(),
        &ORIGIN[1..]
    ));
    out.push_str(&format!(
        "Check a payload first, without creating a review:\n\n```sh\n{}\n```\n\n",
        CHECK.replace("<plugin>", plugin)
    ));
    out.push_str(&format!(
        "Files go beside the payload for a plugin that takes them ({}).\n\n",
        FILES.trim_start_matches("for a plugin with `attachments`, ")
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

/// `pinrail plugins check` as markdown: the verdict, then each reason.
pub fn verdict(verdict: &Value, dir: &str) -> String {
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let mut out = if verdict["usable"] == true {
        format!(
            "{} {} in {dir}: the app would take it.\n",
            text(&verdict["name"]),
            text(&verdict["release"])
        )
    } else {
        format!("{dir}: the app would refuse it.\n")
    };
    for p in verdict["problems"].as_array().into_iter().flatten() {
        out.push_str(&format!("\n- refused: {}", text(&p["message"])));
    }
    for w in verdict["warnings"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "\n- `{}` dropped: {}",
            text(&w["key"]),
            text(&w["message"])
        ));
    }
    if verdict["problems"]
        .as_array()
        .is_some_and(|a| !a.is_empty())
        || verdict["warnings"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    {
        out.push('\n');
    }
    out
}

/// `pinrail plugins` as markdown: how many, then a plugin a line, its
/// version, where it is from and whether it works, and under it what it is.
pub fn listing(listed: &Value) -> String {
    let rows = listed["plugins"].as_array().map(Vec::as_slice).unwrap_or_default();
    let noun = if rows.len() == 1 { "plugin" } else { "plugins" };
    let mut out = format!(
        "{} {noun} installed; pinrail plugins describe says when to use each.\n\n",
        rows.len()
    );
    for plugin in rows {
        let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
        let install = &plugin["install"];
        let from = if install.is_null() {
            "built in".to_string()
        } else if install["linked"] == true {
            format!("linked, {}", text(&install["source"]))
        } else {
            text(&install["source"])
        };
        let state = match plugin["error"].as_str() {
            Some(error) => format!("broken: {error}"),
            None => "ready".to_string(),
        };
        out.push_str(&format!(
            "- {} · {} · {from} · {state}\n",
            text(&plugin["name"]),
            text(&plugin["release"]),
        ));
        if let Some(about) = plugin["description"].as_str().filter(|d| !d.trim().is_empty()) {
            out.push_str(&format!("  {}\n", about.trim()));
        }
    }
    out
}

/// `- **list** (Action list): use when …. Takes files: .glb.`
fn index_line(out: &mut String, plugin: &Value) {
    let s = summary(plugin);
    let name = s["name"].as_str().unwrap_or_default();
    out.push_str(&format!("- **{name}**"));
    if let Some(title) = s["title"].as_str().filter(|t| *t != name) {
        out.push_str(&format!(" ({title})"));
    }
    let when = s["use_when"].as_str();
    if when.is_some() || s["files"].is_array() {
        out.push(':');
    }
    if let Some(when) = when {
        out.push_str(&format!(" {when}"));
    }
    if let Some(files) = s["files"].as_array() {
        // extensions say it shortest; media types only when there are none
        let kinds: Vec<&str> = files.iter().filter_map(Value::as_str).collect();
        let extensions: Vec<&str> = kinds
            .iter()
            .copied()
            .filter(|k| k.starts_with('.'))
            .collect();
        let shown = if extensions.is_empty() {
            kinds
        } else {
            extensions
        };
        out.push_str(&format!(" Takes files: {}.", shown.join(", ")));
    }
    out.push('\n');
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
    if let Some(files) = plugin["attachments"].as_object() {
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
            "\n### Files\n\nTakes files beside the payload: {}{}. The payload schema above says where each goes, as `{{\"$attachment\": \"<name>\"}}`; send each file it names with `--attach PATH[=NAME]`.\n\n```sh\npinrail submit {name} --title \"<what it is about>\" --data payload.json --attach <file> --wait\n```\n",
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
