//! The markdown the command prints by default, for what is not a review (a
//! review's markdown is the server's): a listing, a review's rounds and
//! events, its files, what a plugin command did. Short lines, every id as
//! it is, nothing the JSON has that an agent needs left out.

use serde_json::Value;

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or_default()
}

/// An ISO time as a clock on this machine reads it, `2026-09-16 09:14`, as
/// a review's own markdown prints it; as it came when it does not parse.
fn when(v: &Value) -> String {
    let iso = text(v);
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| iso.to_string())
}

/// What a listing was narrowed to, to say above it.
pub struct Scope {
    pub status: Option<String>,
    pub repo: Option<String>,
    /// narrowed by the defaults rather than the flags given
    pub narrowed: bool,
}

/// A listing: how many of how many and of what, then a review a line, then
/// how to see more.
pub fn listing(rows: &Value, page: &Value, scope: &Scope) -> String {
    let shown = rows.as_array().map_or(0, Vec::len);
    let total = page["total"].as_u64().unwrap_or(shown as u64);
    let noun = if total == 1 { "review" } else { "reviews" };
    let what = match &scope.status {
        Some(status) => format!("{} {noun}", status.replace(',', " or ")),
        None => noun.to_string(),
    };
    let place = scope
        .repo
        .as_deref()
        .map(|r| format!(" in {}", if r == "-" { "no project" } else { r }))
        .unwrap_or_default();
    let mut out = if shown as u64 == total {
        format!("{total} {what}{place}")
    } else {
        format!("{shown} of {total} {what}{place}, newest first")
    };
    if scope.narrowed {
        out.push_str("; --all for every review");
    }
    out.push_str(if shown == 0 { ".\n" } else { ".\n\n" });
    out.push_str(&reviews(rows));
    if page["has_more"] == true
        && let Some(next) = page["next_cursor"].as_str()
    {
        out.push_str(&format!("\nMore: --cursor {next}\n"));
    }
    out
}

/// `- r_… · pending · list · Title · 2 major · acme/api@main · …`, one
/// review a line, with what it asks while pending and what was decided
/// once it ended, as its plugin sums it up.
pub fn reviews(reviews: &Value) -> String {
    let mut out = String::new();
    for r in reviews.as_array().into_iter().flatten() {
        let mut line = format!(
            "- {} · {} · {} · {}",
            text(&r["id"]),
            text(&r["status"]),
            text(&r["plugin"]),
            text(&r["title"])
        );
        let summary = if r["status"] == "pending" {
            &r["summary"]
        } else {
            &r["decision"]["summary"]
        };
        if let Some(summary) = summary_line(summary) {
            line.push_str(&format!(" · {summary}"));
        }
        if let Some(repo) = r["origin"]["repo"].as_str() {
            line.push_str(&format!(" · {repo}"));
            if let Some(reference) = r["origin"]["ref"].as_str() {
                line.push_str(&format!("@{reference}"));
            }
        }
        line.push_str(&format!(" · {}", when(&r["created_at"])));
        if let Some(revises) = r["revises"].as_str() {
            line.push_str(&format!(" · revises {revises}"));
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// A summary in a line: the verdict, then each count, as "approved, 3
/// scheduled" or "2 major, 1 nit". None when there is nothing in it.
fn summary_line(summary: &Value) -> Option<String> {
    let verdict = summary["verdict"]["label"].as_str().map(str::to_string);
    let counts = summary["counts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| Some(format!("{} {}", c["count"].as_u64()?, c["label"].as_str()?)));
    let parts: Vec<String> = verdict.into_iter().chain(counts).collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// The rounds of a review, oldest first.
pub fn rounds(rounds: &Value) -> String {
    let mut out = String::new();
    for (i, r) in rounds.as_array().into_iter().flatten().enumerate() {
        out.push_str(&format!(
            "{}. {} · {} · {} · {}\n",
            i + 1,
            text(&r["id"]),
            text(&r["status"]),
            text(&r["title"]),
            when(&r["created_at"])
        ));
    }
    out
}

/// A review's event log, a line an event.
pub fn events(events: &Value) -> String {
    let mut out = String::new();
    for e in events.as_array().into_iter().flatten() {
        let mut line = format!("- {} {}", when(&e["at"]), text(&e["kind"]));
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
    // pid and start time when server.json has them for this server
    let mut about = Vec::new();
    if let Some(pid) = info["pid"].as_u64() {
        about.push(format!("pid {pid}"));
    }
    if info["started_at"].is_string() {
        about.push(format!("since {}", when(&info["started_at"])));
    }
    match about.is_empty() {
        true => format!("Pinrail's server: {}\n", text(&info["url"])),
        false => format!(
            "Pinrail's server: {} ({})\n",
            text(&info["url"]),
            about.join(", ")
        ),
    }
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
    if let Some(name) = plugin["plugin"]
        .as_str()
        .filter(|_| plugin.get("install").is_some())
    {
        let from = plugin["install"]["source"]
            .as_str()
            .map(|s| format!(" from {s}"))
            .unwrap_or_default();
        if value["state"] == "rolled_back" {
            return format!("{name}: rolled back to {}\n", text(&plugin["version"]));
        }
        if value["state"] == "updated" {
            return format!("{name}: updated to {}{from}\n", text(&plugin["version"]));
        }
        return format!("Installed {name} {}{from}.\n", text(&plugin["version"]));
    }
    match (
        value["state"].as_str(),
        value["removed"].as_str(),
        value["count"].as_u64(),
    ) {
        (Some("up_to_date"), ..) => format!(
            "{}: up to date, {}\n",
            text(&value["name"]),
            text(&value["version"])
        ),
        (Some("failed"), ..) => format!(
            "{}: failed: {}\n",
            text(&value["name"]),
            text(&value["error"])
        ),
        (Some("built_in"), ..) => {
            format!(
                "{}: comes with the app and is updated with it\n",
                text(&value["name"])
            )
        }
        (Some("linked"), ..) => format!(
            "{}: linked, served live from {}\n",
            text(&value["name"]),
            text(&value["source"])
        ),
        (_, Some(name), ..) => match value["restored"].as_str() {
            Some(version) => format!("Removed the link: {name} {version} is back.\n"),
            None => format!("Removed {name}.\n"),
        },
        (_, _, Some(count)) => format!("Reloaded {count} plugins.\n"),
        _ => format!("{value}\n"),
    }
}

/// A new plugin: where it was written, whether the app serves it, and the
/// steps from there.
pub fn scaffolded(written: &Value) -> String {
    let mut out = format!(
        "{} written to {}.\n",
        text(&written["name"]),
        text(&written["dir"])
    );
    if written["linked"] == true {
        out.push_str("Linked: the app serves the folder live.\n");
    }
    out.push_str("\nNext:\n");
    for (i, step) in written["next"].as_array().into_iter().flatten().enumerate() {
        out.push_str(&format!("  {}. {}\n", i + 1, text(step)));
    }
    out.push_str(&format!(
        "\nHow a plugin works: {}\n",
        text(&written["docs"])
    ));
    out
}

/// A dry run that passed.
pub fn valid(answer: &Value) -> String {
    format!(
        "Valid: {} {} would render it.\n",
        text(&answer["plugin"]),
        text(&answer["plugin_version"])
    )
}

/// Why the app refused a request: its message, then each violation's place
/// and reason, a line each.
pub fn refusal(body: &Value) -> String {
    let violations = body["violations"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    if violations.is_empty() {
        let message = body["message"]
            .as_str()
            .unwrap_or("the server refused the request");
        return format!("pinrail: refused: {message}\n");
    }
    let mut out = String::from("pinrail: refused:\n");
    for v in violations {
        let path = v["path"].as_str().filter(|p| !p.is_empty()).unwrap_or("/");
        out.push_str(&format!("  {path}: {}\n", text(&v["message"])));
    }
    out
}
