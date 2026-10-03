//! `pinrail plugins describe <name>`: what an agent needs to ask with one
//! plugin, its payload schema, an example and the files it takes. Choosing
//! one is `pinrail plugins`, a line a plugin with when to use it; how to ask
//! is `pinrail docs`.

use serde_json::Value;

const SUBMIT: &str =
    "pinrail submit <plugin> --title \"<what it is about>\" --data payload.json --wait";

/// As markdown: the plugin as a document, its heading the title, then how
/// to send it; the rest, exit codes included, is `pinrail docs asking`. The
/// decision's schema is left to `--decision-schema`: the decision reads as
/// markdown, and only an agent processing its JSON needs it.
pub fn markdown(plugin: &Value) -> String {
    let mut section = String::new();
    plugin_section(&mut section, plugin);
    let mut out = section
        .trim_start()
        .replacen("## ", "# ", 1)
        .replace("\n### ", "\n## ");
    let name = plugin["name"].as_str().unwrap_or("<plugin>");
    out.push_str(&format!(
        "\n## Submitting\n\n```sh\n{}\n```\n\nInside a git checkout, the command fills in the project from git. Outside one, add `--origin repo=<project>`.\n\nExit codes, rounds and the rest: `pinrail docs asking`.\n",
        SUBMIT.replace("<plugin>", name)
    ));
    out
}

/// `pinrail plugins check` as markdown: the verdict, then each reason.
pub fn verdict(verdict: &Value, dir: &str) -> String {
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let mut out = if verdict["usable"] == true {
        format!(
            "{} {} in {dir}: the app would take it.\n",
            text(&verdict["name"]),
            text(&verdict["version"])
        )
    } else {
        format!("{dir}: the app would refuse it.\n")
    };
    if let Some(hash) = verdict["bundle"]["hash"].as_str() {
        out.push_str(&format!(
            "\nBundle {hash}: {} files, {} bytes.\n",
            verdict["bundle"]["files"], verdict["bundle"]["size"]
        ));
    }
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
    for n in verdict["notes"].as_array().into_iter().flatten() {
        out.push_str(&format!("\n- note: {}", text(&n["message"])));
    }
    let since = &verdict["since"];
    let breaks = since["breaks"].as_array().cloned().unwrap_or_default();
    if since["claims_compatible"] == true {
        for b in &breaks {
            out.push_str(&format!(
                "\n- breaks {}: {}: {}",
                text(&since["previous"]),
                text(&b["path"]),
                text(&b["message"])
            ));
        }
        if !breaks.is_empty() {
            out.push_str(&format!(
                "\n\nRelease it as {}, a version that announces a breaking change.\n",
                text(&since["next"])
            ));
        }
    } else if since.is_object() {
        out.push_str(&format!(
            "\n- note: {} announces a breaking change, so it may change what {} took",
            text(&since["version"]),
            text(&since["previous"])
        ));
    }
    if verdict["problems"]
        .as_array()
        .is_some_and(|a| !a.is_empty())
        || verdict["warnings"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
        || verdict["notes"].as_array().is_some_and(|a| !a.is_empty())
        || (since.is_object() && since["claims_compatible"] != true)
    {
        out.push('\n');
    }
    out
}

/// `pinrail plugins` as markdown: how many, then a plugin a line, its
/// version, where it is from and whether it works, and under it what it is.
pub fn listing(listed: &Value) -> String {
    let rows = listed["plugins"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let noun = if rows.len() == 1 { "plugin" } else { "plugins" };
    let mut out = format!(
        "{} {noun} installed. pinrail plugins describe <name> shows a plugin's payload schema and an example, and --decision-schema shows what it returns.\n\n",
        rows.len()
    );
    for plugin in rows {
        let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
        let install = &plugin["install"];
        let from = if install["source_kind"] == "app" {
            "comes with the app".to_string()
        } else if install["link"] == true {
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
            text(&plugin["version"]),
        ));
        if let Some(about) = plugin["description"]
            .as_str()
            .filter(|d| !d.trim().is_empty())
        {
            out.push_str(&format!("  {}\n", about.trim()));
        }
        if let Some(when) = plugin["use_when"].as_str().filter(|w| !w.trim().is_empty()) {
            out.push_str(&format!("  Use when: {}\n", when.trim()));
        }
        if let Some(kinds) = files(plugin) {
            out.push_str(&format!("  Takes files: {kinds}.\n"));
        }
    }
    out
}

/// The kinds of file a plugin takes beside its payload: extensions, which
/// say it shortest, or media types when it names none.
fn files(plugin: &Value) -> Option<String> {
    let kinds: Vec<&str> = plugin["attachments"]["accept"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
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
    Some(shown.join(", "))
}

fn plugin_section(out: &mut String, plugin: &Value) {
    let text = |key: &str| plugin[key].as_str().filter(|s| !s.is_empty());
    let name = text("plugin").or(text("name")).unwrap_or_default();
    out.push_str(&format!(
        "\n## {} (`{name}`) · {}\n\n",
        text("title").unwrap_or(name),
        text("version").unwrap_or_default()
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
            limits.push(format!("up to {} each", crate::attachments::human(size)));
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
        "\n### Decision\n\nThe decision comes back as markdown to read. To process it as JSON (`--json`), its schema: `pinrail plugins describe {} --decision-schema`.\n",
        name
    ));
}

fn json_block(value: &Value) -> String {
    format!(
        "```json\n{}\n```\n",
        serde_json::to_string_pretty(value).unwrap_or_default()
    )
}
