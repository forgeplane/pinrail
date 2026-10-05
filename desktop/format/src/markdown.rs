//! A review as markdown: the view an agent reads in a session, where JSON
//! is noise. JSON stays the contract; this is a rendering of the same
//! record, in a fixed order, that says what was decided and why.
//!
//! The head is the same for every plugin: the title, a line placing the
//! review, the outcome with who and when, the person's note; what was
//! decided is the plugin's to say, below. The
//! body is the decision data rendered by a generic rule keyed on the
//! vocabulary the plugins share (`decisions`, `action`, `note`, `file`,
//! `line`, `undecided`…), so nothing is lost silently and anything
//! recognised reads as prose.

use chrono::{DateTime, FixedOffset, Local, Utc};
use serde_json::{Map, Value, json};

/// A summary in a line: the verdict, then each count, as "approved, 3
/// scheduled" or "2 accepted, 1 rejected". None when there is nothing in it.
pub fn summary_line(summary: &Value) -> Option<String> {
    let verdict = summary["verdict"]["label"].as_str().map(str::to_string);
    let counts = summary["counts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| Some(format!("{} {}", c["count"].as_u64()?, c["label"].as_str()?)));
    let parts: Vec<String> = verdict.into_iter().chain(counts).collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// How a review's markdown opens.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Head {
    /// A document: the title as a heading, then where it sits and how it
    /// ended; what *Copy as markdown* gives.
    Document,
    /// Command output: the id, status and title on the first line, where it
    /// sits and how it ended on the second; what `pinrail` prints.
    Command,
}

/// Renders a review, as `Review::to_json` shapes it. `round` is `(n, of)`
/// when the review is one of a chain; `template` is the plugin's own
/// rendering of the body, when it declares one. Times read in the local
/// zone.
pub fn render(review: &Value, round: Option<(usize, usize)>, template: Option<&str>) -> String {
    render_in(review, round, template, None, Head::Document)
}

/// `render`, with times in `zone` rather than the local one (the fixture
/// tests pass UTC so their expected files hold anywhere), and the `head`
/// the caller wants.
pub fn render_in(
    review: &Value,
    round: Option<(usize, usize)>,
    template: Option<&str>,
    zone: Option<FixedOffset>,
    head: Head,
) -> String {
    let when = |iso: Option<&str>| when_in(iso, zone);
    let title = review["title"].as_str().unwrap_or("Review");
    let id = review["id"].as_str().unwrap_or("");

    // where it sits: plugin · repo · workflow · ref · round n of m
    let origin = &review["origin"];
    let mut place: Vec<String> = vec![review["plugin"].as_str().unwrap_or("").to_string()];
    for key in ["repo", "workflow", "ref"] {
        if let Some(v) = origin[key].as_str().filter(|s| !s.is_empty()) {
            place.push(v.to_string());
        }
    }
    if let Some((n, of)) = round {
        match review["revises"].as_str().filter(|_| head == Head::Command) {
            Some(revises) => place.push(format!("round {n} of {of}, revises {revises}")),
            None => place.push(format!("round {n} of {of}")),
        }
    }

    // how it stands, in a line
    let status = review["status"].as_str().unwrap_or("pending");
    let data = &review["decision"]["data"];
    let standing = match status {
        "decided" => {
            let by = review["decision"]["decided_by"]
                .as_str()
                .unwrap_or("someone");
            format!(
                "Decided by {by} at {}",
                when(review["decision"]["decided_at"].as_str())
            )
        }
        "withdrawn" => {
            let mut line = format!(
                "Withdrawn by the agent at {}",
                when(review["withdrawn_at"].as_str())
            );
            if let Some(reason) = text(&review["withdrawn_reason"]) {
                line.push_str(&format!(": {reason}"));
            }
            line
        }
        "discarded" => {
            let by = review["discarded_by"].as_str().unwrap_or("someone");
            let mut line = format!(
                "Discarded by {by} at {}",
                when(review["discarded_at"].as_str())
            );
            if let Some(reason) = text(&review["discarded_reason"]) {
                line.push_str(&format!(": {reason}"));
            }
            line
        }
        "expired" => format!("Expired at {}", when(review["expires_at"].as_str())),
        _ => format!("Pending since {}", when(review["created_at"].as_str())),
    };
    let url = origin["url"].as_str().filter(|s| !s.is_empty());
    // what was decided, as the plugin sums it up
    let outcome = summary_line(&review["decision"]["summary"]);

    let mut out = String::new();
    match head {
        Head::Document => {
            out.push_str(&format!("# {title}\n\n{}\n{standing}\n", place.join(" · ")));
            if let Some(outcome) = &outcome {
                out.push_str(&format!("Outcome: {outcome}\n"));
            }
            if status == "pending" {
                if let Some(url) = url {
                    out.push_str(&format!("{url}\n"));
                }
                out.push_str("\nWaiting for a decision.\n");
                return out;
            }
        }
        Head::Command => {
            let mut lower = standing.clone();
            if let Some(first) = lower.get_mut(0..1) {
                first.make_ascii_lowercase();
            }
            out.push_str(&format!(
                "{id} · {status} · {title}\n{} · {lower}\n",
                place.join(" · ")
            ));
            if let Some(outcome) = &outcome {
                out.push_str(&format!("outcome: {outcome}\n"));
            }
            if let Some(url) = url {
                out.push_str(&format!("{url}\n"));
            }
            // the command says where to see a pending review and how to
            // wait on it; the body comes with a decision
            if status == "pending" {
                return out;
            }
        }
    }

    // the other endings say why and stop; a decision carries the note and
    // the data
    if status != "decided" {
        return out;
    }
    if let Some(note) = text(&review["agent_note"]) {
        out.push('\n');
        for line in note.lines() {
            out.push_str(&format!("> {line}\n"));
        }
    }
    // the plugin's own body when it has one and it renders; the generic
    // one otherwise, so a template that fails at runtime costs nothing
    let body = template
        .and_then(|t| match render_template(t, review) {
            Ok(body) => Some(body),
            Err(error) => {
                // said in the log, for the plugin's author to find
                eprintln!(
                    "pinrail: the {} plugin's decision template failed, so the review was rendered without it: {error}",
                    review["plugin"].as_str().unwrap_or("unknown")
                );
                None
            }
        })
        .unwrap_or_else(|| render_data(data));
    if !body.is_empty() {
        out.push('\n');
        out.push_str(body.trim_start_matches('\n'));
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// The body through the plugin's template. The context: `review` (the
/// envelope with its payload), `decision`, `data` (the decision's data),
/// `note`, and `items`: every object in the decision's arrays, each with
/// `payload` set to the payload object of the same id, found at any depth,
/// when there is one.
/// How much work one render of a plugin's template may do, in the engine's
/// units: far more than any decision needs, and a limit on a template that
/// would otherwise run for ever and hold up the server.
const TEMPLATE_FUEL: u64 = 10_000_000;
/// The most text a template may render.
const TEMPLATE_OUTPUT: usize = 1024 * 1024;

fn render_template(source: &str, review: &Value) -> Result<String, String> {
    let mut env = minijinja::Environment::new();
    env.set_fuel(Some(TEMPLATE_FUEL));
    // `{{ item.action | verb }}`: accept → accepted, the way the generic body says it
    env.add_filter("verb", |v: String| verb(&v).unwrap_or(v));
    env.add_template("decision", source)
        .map_err(|e| e.to_string())?;
    let data = review["decision"]["data"].clone();
    let mut payload_items: Vec<&Value> = Vec::new();
    with_ids(&review["payload"], &mut payload_items);
    let mut items: Vec<Value> = Vec::new();
    if let Some(map) = data.as_object() {
        for value in map.values() {
            let Some(list) = value.as_array() else {
                continue;
            };
            for item in list.iter().filter(|i| i.is_object()) {
                let mut joined: Map<String, Value> = item.as_object().cloned().unwrap_or_default();
                let payload = item
                    .get("id")
                    .and_then(|id| payload_items.iter().find(|p| p.get("id") == Some(id)))
                    .map(|p| (*p).clone())
                    .unwrap_or(Value::Null);
                joined.insert("payload".into(), payload);
                items.push(Value::Object(joined));
            }
        }
    }
    let context = json!({
        "review": review,
        "decision": review["decision"],
        "data": data,
        "note": review["agent_note"],
        "items": items,
    });
    let text = env
        .get_template("decision")
        .map_err(|e| e.to_string())?
        .render(minijinja::Value::from_serialize(&context))
        .map_err(|e| e.to_string())?;
    if text.len() > TEMPLATE_OUTPUT {
        return Err(format!(
            "the template rendered more than {TEMPLATE_OUTPUT} bytes"
        ));
    }
    Ok(text)
}

/// Every object with an `id` in the arrays of `value`, at any depth, in
/// document order, so the first of two with one id is the one found.
fn with_ids<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => map.values().for_each(|v| with_ids(v, out)),
        Value::Array(list) => {
            for item in list {
                if item.get("id").is_some() {
                    out.push(item);
                }
                with_ids(item, out);
            }
        }
        _ => {}
    }
}

/// The decision data, by the shared vocabulary. Scalars first, then one
/// section per array of objects, then `undecided`, then anything else as
/// key and JSON so nothing is dropped.
pub fn render_data(data: &Value) -> String {
    let Some(map) = data.as_object() else {
        return String::new();
    };
    let mut out = String::new();
    let mut later: Vec<(&String, &Value)> = Vec::new();

    for (key, value) in map {
        if key == "undecided" || value.is_array() || value.is_object() {
            continue;
        }
        if value.is_null() {
            continue;
        }
        let shown = match value {
            Value::String(s) => verb(s).unwrap_or_else(|| s.clone()),
            other => other.to_string(),
        };
        out.push_str(&format!("{}: {shown}\n", humanise(key)));
    }

    for (key, value) in map {
        if key == "undecided" {
            continue;
        }
        match value {
            Value::Array(items) if items.iter().all(Value::is_object) && !items.is_empty() => {
                out.push_str(&format!("\n## {}\n\n", humanise(key)));
                for item in items {
                    out.push_str(&render_item(item, 0));
                }
            }
            Value::Array(items) if items.is_empty() => {}
            Value::Array(_) | Value::Object(_) => later.push((key, value)),
            _ => {}
        }
    }

    if let Some(ids) = map.get("undecided").and_then(Value::as_array)
        && !ids.is_empty()
    {
        let ids: Vec<String> = ids.iter().map(id_of).collect();
        out.push_str(&format!("\nUndecided: {}\n", ids.join(", ")));
    }

    for (key, value) in later {
        out.push_str(&format!("\n{}: {value}\n", humanise(key)));
    }
    out
}

/// One item as a bullet: `#id` and the verb in bold, then a place, then
/// the text; the note as a nested quote; nested arrays as nested bullets.
fn render_item(item: &Value, depth: usize) -> String {
    let indent = "  ".repeat(depth);
    let mut head: Vec<String> = Vec::new();
    if let Some(id) = item.get("id") {
        head.push(match id {
            Value::Number(_) => format!("**#{id}**"),
            Value::String(s) => format!("**`{s}`**"),
            other => format!("**{other}**"),
        });
    }
    if let Some(v) = item["action"].as_str().or(item["verdict"].as_str()) {
        head.push(format!("**{}**", verb(v).unwrap_or_else(|| v.to_string())));
    }
    let mut place: Vec<String> = Vec::new();
    if let Some(file) = item["file"].as_str().or(item["path"].as_str()) {
        match item["line"].as_u64() {
            Some(line) => place.push(format!("`{file}:{line}`")),
            None => place.push(format!("`{file}`")),
        }
    } else if let Some(line) = item["line"].as_u64() {
        place.push(format!("line {line}"));
    }
    if let Some(selector) = item["selector"].as_str() {
        place.push(format!("`{selector}`"));
    }
    if let Some(quote) = item["quote"].as_str() {
        place.push(format!("“{}”", one_line(quote)));
    }
    let body_key = ["title", "subject", "text", "body", "snippet"]
        .into_iter()
        .find(|k| item[*k].as_str().is_some_and(|s| !s.trim().is_empty()));
    let body_text = body_key.and_then(|k| item[k].as_str()).map(str::trim);
    let body = body_text.map(|s| s.lines().next().unwrap_or("").to_string());
    let mut parts: Vec<String> = Vec::new();
    if !head.is_empty() {
        parts.push(head.join(" "));
    }
    if !place.is_empty() {
        parts.push(place.join(" "));
    }
    let mut line = parts.join(" ");
    if let Some(body) = body {
        if line.is_empty() {
            line = body;
        } else {
            line.push_str(&format!(" — {body}"));
        }
    }
    // an object with none of the known keys: say what it holds
    let raw = line.is_empty();
    if raw {
        line = item.to_string();
    }
    let mut out = format!("{indent}- {line}\n");
    // the rest of a long body, then every field the line did not show, so
    // nothing the person said is lost
    for l in body_text.into_iter().flat_map(|s| s.lines().skip(1)) {
        match l.trim() {
            "" => out.push('\n'),
            _ => out.push_str(&format!("{indent}  {l}\n")),
        }
    }
    let shown = [
        "id", "action", "verdict", "file", "path", "line", "selector", "quote", "note", "edits",
        "comments",
    ];
    if let Some(map) = item.as_object().filter(|_| !raw) {
        for (key, value) in map {
            if shown.contains(&key.as_str()) || Some(key.as_str()) == body_key || value.is_null() {
                continue;
            }
            let value = match value {
                Value::String(s) if s.trim().is_empty() => continue,
                Value::String(s) => s
                    .trim()
                    .lines()
                    .map(|l| match l.trim() {
                        "" => String::new(),
                        _ => format!("{indent}    {l}"),
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                    .trim_start()
                    .to_string(),
                other => other.to_string(),
            };
            out.push_str(&format!("{indent}  {}: {value}\n", humanise(key)));
        }
    }
    if let Some(note) = item["note"].as_str().filter(|s| !s.trim().is_empty()) {
        for l in note.lines() {
            out.push_str(&format!("{indent}  > {l}\n"));
        }
    }
    if let Some(edits) = item["edits"].as_array().filter(|e| !e.is_empty()) {
        for edit in edits {
            let from = one_line(edit["from"].as_str().unwrap_or(""));
            let to = one_line(edit["to"].as_str().unwrap_or(""));
            let shown = match (from.is_empty(), to.is_empty()) {
                (true, _) => format!("added “{to}”"),
                (_, true) => format!("cut “{from}”"),
                _ => format!("“{from}” → “{to}”"),
            };
            out.push_str(&format!("{indent}  - {shown}\n"));
        }
    }
    if let Some(comments) = item["comments"].as_array().filter(|c| !c.is_empty()) {
        for c in comments {
            out.push_str(&render_item(c, depth + 1));
        }
    }
    out
}

/// An action or verdict as the past participle it reads as.
fn verb(v: &str) -> Option<String> {
    Some(
        match v {
            "accept" | "accepted" | "keep" => "accepted",
            "reject" | "rejected" | "decline" => "rejected",
            "send" | "sent" => "sent",
            "revise" | "revised" => "revised",
            "discard" | "discarded" => "discarded",
            "approve" | "approved" => "approved",
            "request_changes" | "changes_requested" | "changes" => "changes requested",
            "edit" | "edited" => "edited",
            "skip" | "skipped" => "skipped",
            _ => return None,
        }
        .to_string(),
    )
}

fn humanise(key: &str) -> String {
    let mut s = key.replace('_', " ");
    if let Some(first) = s.get(0..1) {
        s = first.to_uppercase() + &s[1..];
    }
    s
}

fn id_of(v: &Value) -> String {
    match v {
        Value::Number(_) => format!("#{v}"),
        Value::String(s) => format!("`{s}`"),
        other => other.to_string(),
    }
}

fn text(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Every line of `s`, on one.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// An ISO time as a clock reads it, `2026-09-16 09:14`: the local clock,
/// or the zone given.
fn when_in(iso: Option<&str>, zone: Option<FixedOffset>) -> String {
    iso.and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| match zone {
            Some(z) => t.with_timezone(&z).format("%Y-%m-%d %H:%M").to_string(),
            None => t.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string(),
        })
        .or_else(|| iso.map(str::to_string))
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d %H:%M").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn decided(plugin: &str, data: Value) -> Value {
        json!({
            "id": "r_1", "plugin": plugin, "plugin_version": 1, "title": "Dedup tickets on save",
            "origin": {"repo": "acme/api", "workflow": "pr-review", "ref": "42", "url": "https://x/42"},
            "created_at": "2026-09-16T08:00:00Z", "status": "decided",
            "decision": {"decided_by": "pnezis", "decided_at": "2026-09-16T09:14:00Z", "data": data},
            "agent_note": "Reversing is a no-op here either way."
        })
    }

    #[test]
    fn the_outcome_reads_under_who_decided() {
        let mut review = decided("calendar", json!({"verdict": "approve"}));
        review["decision"]["summary"] = json!({
            "verdict": {"label": "approved", "tone": "success"},
            "counts": [{"label": "scheduled", "count": 3, "tone": "success"}]
        });
        let md = render(&review, None, None);
        let lines: Vec<&str> = md.lines().collect();
        assert!(lines[3].starts_with("Decided by pnezis at "), "{md}");
        assert_eq!(lines[4], "Outcome: approved, 3 scheduled", "{md}");

        // a plugin that declares no outcome summary has no line for it
        let md = render(
            &decided("calendar", json!({"verdict": "approve"})),
            None,
            None,
        );
        assert!(!md.contains("Outcome"), "{md}");
    }

    #[test]
    fn a_code_review_decision_reads_in_order() {
        let review = decided(
            "review",
            json!({
                "decisions": [
                    {"id": 17, "action": "accept"},
                    {"id": 18, "action": "reject", "note": "we never retry writes"},
                    {"id": 19, "action": "accept"}
                ],
                "comments": [{"file": "lib/acme/tickets.ex", "line": 151, "body": "please add a test for the empty list"}],
                "general_comments": [{"body": "The bucket column needs a backfill note."}],
                "undecided": []
            }),
        );
        let md = render(&review, Some((2, 2)), None);
        let lines: Vec<&str> = md.lines().collect();
        assert_eq!(lines[0], "# Dedup tickets on save");
        assert_eq!(
            lines[2],
            "review · acme/api · pr-review · 42 · round 2 of 2"
        );
        assert!(lines[3].starts_with("Decided by pnezis at "), "{md}");
        assert!(
            md.contains("> Reversing is a no-op here either way.\n"),
            "{md}"
        );
        assert!(md.contains("\n## Decisions\n\n- **#17** **accepted**\n- **#18** **rejected**\n  > we never retry writes\n- **#19** **accepted**\n"), "{md}");
        assert!(md.contains("\n## Comments\n\n- `lib/acme/tickets.ex:151` — please add a test for the empty list\n"), "{md}");
        assert!(
            md.contains("\n## General comments\n\n- The bucket column needs a backfill note.\n"),
            "{md}"
        );
        assert!(
            !md.contains("Undecided"),
            "an empty undecided prints nothing: {md}"
        );
    }

    #[test]
    fn a_list_decision_counts_and_names_the_undecided() {
        let review = decided(
            "list",
            json!({
                "decisions": [{"id": 1, "action": "accept"}, {"id": 3, "action": "reject", "note": "not ours"}],
                "additions": [{"group": "Open", "body": "Also rotate the key."}],
                "undecided": [2, 4, 5]
            }),
        );
        let md = render(&review, None, None);
        assert!(
            md.contains("\n## Additions\n\n- Also rotate the key.\n"),
            "{md}"
        );
        assert!(md.contains("\nUndecided: #2, #4, #5\n"), "{md}");
        assert!(!md.contains("round"), "{md}");
    }

    #[test]
    fn an_email_decision_shows_edits_and_passage_notes() {
        let review = decided(
            "email",
            json!({
                "drafts": [
                    {"id": "northwind", "action": "send", "subject": "Your Acme renewal on 12 October", "body": "Hi Priya,\n\nlong body",
                     "edits": [{"from": "at your earliest convenience", "to": "this week"}],
                     "comments": [{"quote": "I wanted to reach out", "note": "we never say reach out"}]},
                    {"id": "kestrel", "action": "discard", "subject": "Overdue invoice", "note": "finance handles this one"}
                ],
                "undecided": ["brightside"]
            }),
        );
        let md = render(&review, None, None);
        assert!(md.contains("- **`northwind`** **sent** — Your Acme renewal on 12 October\n  Body: Hi Priya,\n\n    long body\n  - “at your earliest convenience” → “this week”\n  - “I wanted to reach out”\n    > we never say reach out\n"), "{md}");
        assert!(
            md.contains(
                "- **`kestrel`** **discarded** — Overdue invoice\n  > finance handles this one\n"
            ),
            "{md}"
        );
        assert!(md.contains("\nUndecided: `brightside`\n"), "{md}");
    }

    #[test]
    fn an_artifact_decision_leads_with_the_verdict_and_points_at_elements() {
        let review = decided(
            "artifact",
            json!({
                "verdict": "request_changes",
                "comments": [{"id": "c1", "kind": "element", "selector": "#features > div:nth-of-type(2) > h3", "tag": "h3", "text": "too small on mobile", "snippet": "<h3>Draft close</h3>"}]
            }),
        );
        let md = render(&review, None, None);
        assert!(md.contains("\nVerdict: changes requested\n"), "{md}");
        assert!(
            md.contains("- **`c1`** `#features > div:nth-of-type(2) > h3` — too small on mobile\n"),
            "{md}"
        );
    }

    #[test]
    fn the_other_endings_say_why_and_stop() {
        let mut review = decided("list", json!({}));
        review["status"] = json!("withdrawn");
        review["decision"] = Value::Null;
        review["withdrawn_at"] = json!("2026-09-16T09:00:00Z");
        review["withdrawn_reason"] = json!("the branch was force-pushed");
        let md = render(&review, None, None);
        assert!(md.contains("Withdrawn by the agent at "), "{md}");
        assert!(
            md.trim_end().ends_with("the branch was force-pushed"),
            "{md}"
        );

        review["status"] = json!("discarded");
        review["discarded_at"] = json!("2026-09-16T09:00:00Z");
        review["discarded_by"] = json!("pnezis");
        review["discarded_reason"] = json!("wrong branch");
        let md = render(&review, None, None);
        assert!(md.contains("Discarded by pnezis at "), "{md}");
        assert!(md.trim_end().ends_with("wrong branch"), "{md}");

        review["status"] = json!("expired");
        review["expires_at"] = json!("2026-09-16T10:00:00Z");
        let md = render(&review, None, None);
        assert!(md.contains("Expired at "), "{md}");

        review["status"] = json!("pending");
        let md = render(&review, None, None);
        assert!(md.contains("Pending since "), "{md}");
        assert!(md.contains("https://x/42\n"), "{md}");
        assert!(md.trim_end().ends_with("Waiting for a decision."), "{md}");
        assert!(
            !md.contains("Reversing"),
            "the note is not shown while pending: {md}"
        );
    }

    #[test]
    fn the_review_plugins_decided_fixture_renders() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../plugins/review/fixtures/dedup-round-1.decided.json");
        let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let review = json!({
            "id": "r_1", "plugin": "review", "plugin_version": 1, "title": fixture["title"],
            "origin": {"repo": "acme/api", "workflow": "pr-review"},
            "status": "decided", "decision": fixture["decision"], "agent_note": null
        });
        let md = render(&review, Some((1, 2)), None);
        assert!(
            md.contains(
                "review · acme/api · pr-review · round 1 of 2
"
            ),
            "{md}"
        );
        assert!(md.contains("Decided by alice at "), "{md}");
        assert!(
            md.contains(
                "- **#18** **rejected**
  > dont nitpick
"
            ),
            "{md}"
        );
        assert!(
            md.contains(
                "
Undecided: #19, #20
"
            ),
            "{md}"
        );
    }

    #[test]
    fn an_item_finds_its_payload_object_however_deep_the_payload_nests_it() {
        let review = json!({
            "id": "r_1", "plugin": "p", "plugin_version": 1, "title": "Nested", "origin": {},
            "status": "decided",
            "payload": {"groups": [{"title": "g", "items": [{"id": 7, "title": "deep"}]}]},
            "decision": {"decided_by": "alice", "decided_at": "2026-09-10T09:00:00Z",
                         "data": {"decisions": [{"id": 7, "action": "accept"}]}}
        });
        let md = render(
            &review,
            None,
            Some("{% for item in items %}- {{ item.id }} {{ item.payload.title }}\n{% endfor %}"),
        );
        assert!(md.contains("- 7 deep\n"), "{md}");
    }

    #[test]
    fn a_template_that_would_run_for_ever_gives_way_to_the_generic_body() {
        // loops in loops: each range is within the engine's own cap, and
        // together they would never finish
        let endless = "{% for a in range(100000) %}{% for b in range(100000) %}\
                       {% for c in range(100000) %}x{% endfor %}{% endfor %}{% endfor %}";
        let review = decided(
            "list",
            json!({"decisions": [{"id": 1, "action": "accept"}]}),
        );
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(render(&review, None, Some(endless)));
        });
        let text = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the template was still running after 10 seconds");
        assert!(text.contains("- **#1** **accepted**"), "{text}");
        assert!(!text.contains("xxx"), "{text}");
    }

    #[test]
    fn the_review_plugins_template_names_the_proposals_from_the_payload() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/review");
        let fixture: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("fixtures/dedup-round-1.decided.json")).unwrap(),
        )
        .unwrap();
        let template = std::fs::read_to_string(root.join("templates/decision.md.j2")).unwrap();
        crate::compile_template(&template).unwrap();
        let review = json!({
            "id": "r_1", "plugin": "review", "plugin_version": 1, "title": fixture["title"],
            "origin": {"repo": "acme/api"}, "status": "decided",
            "payload": fixture["payload"], "decision": fixture["decision"], "agent_note": "dont nitpick the docs"
        });
        let md = render(&review, None, Some(&template));
        assert!(md.contains("\n## Proposals\n\n- **#18 rejected** `lib/acme/tickets.ex:149` — reversing twice is a no-op with a cost (major)\n  > dont nitpick\n"), "{md}");
        assert!(md.contains("\nUndecided: #19, #20\n"), "{md}");
        assert!(
            !md.contains("## Decisions"),
            "the template replaces the generic body: {md}"
        );

        // a template that fails to compile is reported; one that fails to
        // render falls back to the generic body
        assert!(crate::compile_template("{% if %}").is_err());
        let md = render(&review, None, Some("{{ items | nosuchfilter }}"));
        assert!(md.contains("## Decisions"), "{md}");
    }

    #[test]
    fn unknown_shapes_are_not_dropped() {
        let review = decided(
            "custom",
            json!({
                "mood": "fine",
                "scores": [1, 2, 3],
                "meta": {"k": "v"},
                "things": [{"id": 1, "weird": true}]
            }),
        );
        let md = render(&review, None, None);
        assert!(md.contains("\nMood: fine\n"), "{md}");
        assert!(md.contains("\nScores: [1,2,3]\n"), "{md}");
        assert!(md.contains("\nMeta: {\"k\":\"v\"}\n"), "{md}");
        assert!(md.contains("\n## Things\n\n- **#1**\n"), "{md}");
    }
}
