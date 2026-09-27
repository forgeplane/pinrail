//! Output: JSON on stdout, diagnostics on stderr, the decision file, the
//! export tree.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::api::Client;

static VERBOSE: AtomicBool = AtomicBool::new(false);

pub fn set_verbose(on: bool) {
    VERBOSE.store(on, Ordering::Relaxed);
}

/// A note on what the command did along the way, on stderr with
/// --verbose only; the answer itself is on stdout, and errors and
/// warnings are printed whatever the flag.
pub fn note(message: impl std::fmt::Display) {
    if VERBOSE.load(Ordering::Relaxed) {
        eprintln!("pinrail: {message}");
    }
}

/// Text for a terminal: control characters removed, except newlines and
/// tabs, so text from a review cannot move the cursor, clear the screen,
/// plant links or write the clipboard. JSON needs none of this, since its
/// encoder escapes them.
pub fn terminal_safe(text: &str) -> String {
    text.chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect()
}

pub fn print_json(value: &Value, pretty: bool) {
    let text = if pretty {
        serde_json::to_string_pretty(value).unwrap_or_default()
    } else {
        serde_json::to_string(value).unwrap_or_default()
    };
    println!("{text}");
}

/// A refused request: the server's error body, pretty, on stderr.
pub fn error_json(body: &Value) {
    match body {
        Value::Null => eprintln!("pinrail: the server refused the request"),
        other => eprintln!(
            "pinrail: {}",
            serde_json::to_string_pretty(other).unwrap_or_default()
        ),
    }
}

/// `decision.data` as a 2-space-indented file with a trailing newline, the
/// shape the workflows' publish steps read.
pub fn write_decision(path: &Path, data: &Value) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut text = serde_json::to_string_pretty(data)?;
    text.push('\n');
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

/// "6 kept, 3 declined, 1 edited" for a decision in the editorial shape:
/// `items[].outcome` of keep, decline or edit. None for other shapes.
pub fn editorial_counts(data: &Value) -> Option<String> {
    let items = data.get("items")?.as_array()?;
    let mut kept = 0;
    let mut declined = 0;
    let mut edited = 0;
    for item in items {
        match item.get("outcome").and_then(Value::as_str)? {
            "keep" => kept += 1,
            "decline" => declined += 1,
            "edit" => edited += 1,
            _ => return None,
        }
    }
    let mut parts = vec![format!("{kept} kept"), format!("{declined} declined")];
    if edited > 0 {
        parts.push(format!("{edited} edited"));
    }
    if let Some(undecided) = data.get("undecided").and_then(Value::as_array)
        && !undecided.is_empty()
    {
        parts.push(format!("{} undecided", undecided.len()));
    }
    Some(parts.join(", "))
}

/// Writes every review as `reviews/<id>/review.json` plus `events.jsonl`,
/// newest first through the paged listing. Returns how many were written.
pub fn export(client: &Client, dir: &Path) -> Result<usize> {
    let root = dir.join("reviews");
    std::fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;
    let mut count = 0;
    let mut cursor: Option<String> = None;
    loop {
        let mut query: Vec<(&str, String)> =
            vec![("include_revised", "true".into()), ("limit", "200".into())];
        if let Some(c) = &cursor {
            query.push(("cursor", c.clone()));
        }
        let listing = client.list(&query)?;
        for item in listing["reviews"].as_array().into_iter().flatten() {
            let id = item["id"].as_str().context("review without an id")?;
            let review = client.get_review(id)?;
            let events = client.events(id)?;
            let target = root.join(id);
            std::fs::create_dir_all(&target)?;
            let mut body = serde_json::to_string_pretty(&review)?;
            body.push('\n');
            std::fs::write(target.join("review.json"), body)?;
            let mut log = String::new();
            for event in events.as_array().into_iter().flatten() {
                log.push_str(&serde_json::to_string(event)?);
                log.push('\n');
            }
            std::fs::write(target.join("events.jsonl"), log)?;
            count += 1;
        }
        match listing["next_cursor"].as_str() {
            Some(next) if listing["has_more"] == true => cursor = Some(next.to_string()),
            _ => break,
        }
    }
    Ok(count)
}
