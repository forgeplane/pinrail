//! The markdown the command prints by default, for what is not a review (a
//! review's markdown is the server's): a listing, a review's rounds and
//! events, its files, what a plugin command did. Short lines, every id as
//! it is, nothing the JSON has that an agent needs left out.

use serde_json::Value;

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or_default()
}

/// `- r_… · pending · list · Title · acme/api@main`, one review a line.
pub fn reviews(reviews: &Value) -> String {
    let rows = reviews.as_array().map(Vec::as_slice).unwrap_or_default();
    if rows.is_empty() {
        return "No reviews.\n".into();
    }
    let mut out = String::new();
    for r in rows {
        let mut line = format!(
            "- `{}` · {} · {} · {}",
            text(&r["id"]),
            text(&r["status"]),
            text(&r["plugin"]),
            text(&r["title"])
        );
        if let Some(repo) = r["origin"]["repo"].as_str() {
            line.push_str(&format!(" · {repo}"));
            if let Some(reference) = r["origin"]["ref"].as_str() {
                line.push_str(&format!("@{reference}"));
            }
        }
        line.push_str(&format!(" · {}\n", text(&r["created_at"])));
        out.push_str(&line);
    }
    out
}

/// The rounds of a review, oldest first.
pub fn rounds(rounds: &Value) -> String {
    let mut out = String::new();
    for (i, r) in rounds.as_array().into_iter().flatten().enumerate() {
        out.push_str(&format!(
            "{}. `{}` · {} · {} · {}\n",
            i + 1,
            text(&r["id"]),
            text(&r["status"]),
            text(&r["title"]),
            text(&r["created_at"])
        ));
    }
    out
}

/// A review's event log, a line an event.
pub fn events(events: &Value) -> String {
    let mut out = String::new();
    for e in events.as_array().into_iter().flatten() {
        let mut line = format!("- {} {}", text(&e["at"]), text(&e["kind"]));
        if let Some(actor) = e["actor"].as_str() {
            line.push_str(&format!(" by {actor}"));
        }
        if let Some(attrs) = e["attrs"].as_object().filter(|a| !a.is_empty()) {
            line.push_str(&format!(" {}", Value::Object(attrs.clone())));
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// The files a review carries: name, kind and size.
pub fn attachments(files: &Value) -> String {
    let rows = files.as_array().map(Vec::as_slice).unwrap_or_default();
    if rows.is_empty() {
        return "No files.\n".into();
    }
    rows.iter()
        .map(|f| {
            format!(
                "- `{}` · {} · {} bytes\n",
                text(&f["name"]),
                text(&f["media_type"]),
                f["size"].as_u64().unwrap_or(0)
            )
        })
        .collect()
}

/// Where the server is.
pub fn server(info: &Value) -> String {
    format!(
        "Pinrail's server: {} (pid {}, since {})\n",
        text(&info["url"]),
        info["pid"],
        text(&info["started_at"])
    )
}

/// What a `plugins` command other than the listing did.
pub fn plugins_result(value: &Value) -> String {
    if let Some(all) = value.as_array() {
        return all.iter().map(plugins_result).collect();
    }
    // install, and an update that installed: the plugin as the app has it
    let plugin = if value["plugin"].is_object() {
        &value["plugin"]
    } else {
        value
    };
    if let Some(name) = plugin["name"]
        .as_str()
        .filter(|_| plugin.get("entry").is_some())
    {
        let from = plugin["install"]["source"]
            .as_str()
            .map(|s| format!(" from {s}"))
            .unwrap_or_default();
        let verb = if value["state"] == "updated" {
            "Updated"
        } else {
            "Installed"
        };
        return format!("{verb} `{name}` {}{from}.\n", text(&plugin["release"]));
    }
    match (
        value["state"].as_str(),
        value["removed"].as_str(),
        value["count"].as_u64(),
        value["versions"].as_array(),
    ) {
        (Some("up_to_date"), ..) => format!("Up to date: {}.\n", text(&value["version"])),
        (_, Some(name), ..) => format!("Removed `{name}`.\n"),
        (_, _, Some(count), _) => format!("Reloaded {count} plugins.\n"),
        (_, _, _, Some(versions)) => format!(
            "`{}`: current v{}; reviews render with {}.\n",
            text(&value["name"]),
            value["current"],
            versions
                .iter()
                .map(|v| format!("v{v}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => format!("{value}\n"),
    }
}

/// A dry run that passed.
pub fn valid(answer: &Value) -> String {
    format!(
        "Valid: {} {} would render it.\n",
        text(&answer["plugin"]),
        text(&answer["plugin_release"])
    )
}
