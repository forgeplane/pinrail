//! A review as markdown: the view an agent reads in a session, where JSON
//! is noise. JSON stays the contract; this is a rendering of the same
//! record, in a fixed order, that says what was decided and why.
//!
//! The head is the same for every plugin: the title, a line placing the
//! review, the outcome with who, when and a tally, the person's note. The
//! body is the decision data rendered by a generic rule keyed on the
//! vocabulary the plugins share (`decisions`, `action`, `note`, `file`,
//! `line`, `undecided`…), so nothing is lost silently and anything
//! recognised reads as prose.

use chrono::{DateTime, Local, Utc};
use serde_json::{Map, Value, json};

/// Checks a plugin's template compiles, at load, so a bad one is reported
/// on the plugin's row rather than at the first review.
pub fn compile(source: &str) -> Result<(), String> {
    let mut env = minijinja::Environment::new();
    env.add_template("decision", source)
        .map_err(|e| e.to_string())
}

/// Renders a review, as `Review::to_json` shapes it. `round` is `(n, of)`
/// when the review is one of a chain; `template` is the plugin's own
/// rendering of the body, when it declares one.
pub fn render(review: &Value, round: Option<(usize, usize)>, template: Option<&str>) -> String {
    let mut out = String::new();
    let title = review["title"].as_str().unwrap_or("Review");
    out.push_str(&format!("# {title}\n\n"));

    // where it sits: plugin · repo · workflow · ref · round n of m
    let origin = &review["origin"];
    let mut place: Vec<String> = vec![review["plugin"].as_str().unwrap_or("").to_string()];
    for key in ["repo", "workflow", "ref"] {
        if let Some(v) = origin[key].as_str().filter(|s| !s.is_empty()) {
            place.push(v.to_string());
        }
    }
    if let Some((n, of)) = round {
        place.push(format!("round {n} of {of}"));
    }
    out.push_str(&place.join(" · "));
    out.push('\n');

    let status = review["status"].as_str().unwrap_or("pending");
    let data = &review["decision"]["data"];
    match status {
        "decided" => {
            let by = review["decision"]["decided_by"]
                .as_str()
                .unwrap_or("someone");
            let at = when(review["decision"]["decided_at"].as_str());
            let mut line = format!("Decided by {by} at {at}");
            if let Some(tally) = tally(data) {
                line.push_str(&format!(" · {tally}"));
            }
            out.push_str(&line);
            out.push('\n');
        }
        "withdrawn" => {
            out.push_str(&format!(
                "Withdrawn by the agent at {}",
                when(review["withdrawn_at"].as_str())
            ));
            if let Some(reason) = text(&review["withdrawn_reason"]) {
                out.push_str(&format!(": {reason}"));
            }
            out.push('\n');
        }
        "discarded" => {
            let by = review["discarded_by"].as_str().unwrap_or("someone");
            out.push_str(&format!(
                "Discarded by {by} at {}",
                when(review["discarded_at"].as_str())
            ));
            if let Some(reason) = text(&review["discarded_reason"]) {
                out.push_str(&format!(": {reason}"));
            }
            out.push('\n');
        }
        "expired" => {
            out.push_str(&format!(
                "Expired at {}\n",
                when(review["expires_at"].as_str())
            ));
        }
        _ => {
            out.push_str(&format!(
                "Pending since {}\n",
                when(review["created_at"].as_str())
            ));
            if let Some(url) = origin["url"].as_str().filter(|s| !s.is_empty()) {
                out.push_str(&format!("{url}\n"));
            }
            out.push_str("\nWaiting for a decision.\n");
            return out;
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
        .and_then(|t| render_template(t, review).ok())
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
/// `payload` set to the payload object of the same id, when there is one.
fn render_template(source: &str, review: &Value) -> Result<String, String> {
    let mut env = minijinja::Environment::new();
    env.add_template("decision", source)
        .map_err(|e| e.to_string())?;
    let data = review["decision"]["data"].clone();
    let payload_items: Vec<&Value> = review["payload"]
        .as_object()
        .map(|p| {
            p.values()
                .filter_map(Value::as_array)
                .flatten()
                .filter(|v| v.is_object() && v.get("id").is_some())
                .collect()
        })
        .unwrap_or_default();
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
    env.get_template("decision")
        .map_err(|e| e.to_string())?
        .render(minijinja::Value::from_serialize(&context))
        .map_err(|e| e.to_string())
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
        place.push(format!("“{}”", first_line(quote, 100)));
    }
    let body = ["title", "subject", "text", "body", "snippet"]
        .iter()
        .find_map(|k| item[k].as_str().filter(|s| !s.trim().is_empty()))
        .map(|s| first_line(s, 140));
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
    if line.is_empty() {
        // an object with none of the known keys: say what it holds
        line = item.to_string();
    }
    let mut out = format!("{indent}- {line}\n");
    if let Some(note) = item["note"].as_str().filter(|s| !s.trim().is_empty()) {
        for l in note.lines() {
            out.push_str(&format!("{indent}  > {l}\n"));
        }
    }
    if let Some(edits) = item["edits"].as_array().filter(|e| !e.is_empty()) {
        for edit in edits {
            let from = first_line(edit["from"].as_str().unwrap_or(""), 60);
            let to = first_line(edit["to"].as_str().unwrap_or(""), 60);
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

/// `2 accepted, 1 rejected`, or `approved`, or `4 items`, from the
/// action and verdict values across every array of objects.
fn tally(data: &Value) -> Option<String> {
    let map = data.as_object()?;
    let mut counts: Vec<(String, usize)> = Vec::new();
    let mut items = 0;
    for value in map.values() {
        let Some(list) = value.as_array() else {
            continue;
        };
        for item in list.iter().filter(|i| i.is_object()) {
            items += 1;
            if let Some(v) = item["action"].as_str().or(item["verdict"].as_str()) {
                let word = verb(v).unwrap_or_else(|| v.to_string());
                match counts.iter_mut().find(|(w, _)| *w == word) {
                    Some((_, n)) => *n += 1,
                    None => counts.push((word, 1)),
                }
            }
        }
    }
    if !counts.is_empty() {
        return Some(
            counts
                .iter()
                .map(|(w, n)| format!("{n} {w}"))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    if let Some(v) = map.get("verdict").and_then(Value::as_str) {
        return Some(verb(v).unwrap_or_else(|| v.to_string()));
    }
    (items > 0).then(|| format!("{items} item{}", if items == 1 { "" } else { "s" }))
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

fn first_line(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or("").trim();
    if line.chars().count() > max {
        let cut: String = line.chars().take(max - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        line.to_string()
    }
}

/// An ISO time as the local clock reads it, `2026-09-16 09:14`.
fn when(iso: Option<&str>) -> String {
    iso.and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
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
        assert!(lines[3].ends_with("· 2 accepted, 1 rejected"), "{md}");
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
        assert!(md.contains("· 1 accepted, 1 rejected\n"), "{md}");
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
        assert!(md.contains("· 1 sent, 1 discarded\n"), "{md}");
        assert!(md.contains("- **`northwind`** **sent** — Your Acme renewal on 12 October\n  - “at your earliest convenience” → “this week”\n  - “I wanted to reach out”\n    > we never say reach out\n"), "{md}");
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
        assert!(md.contains("· changes requested\n"), "{md}");
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
                "· 1 rejected
"
            ),
            "{md}"
        );
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
    fn the_review_plugins_template_names_the_proposals_from_the_payload() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/review");
        let fixture: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("fixtures/dedup-round-1.decided.json")).unwrap(),
        )
        .unwrap();
        let template = std::fs::read_to_string(root.join("decision.md.j2")).unwrap();
        compile(&template).unwrap();
        let review = json!({
            "id": "r_1", "plugin": "review", "plugin_version": 1, "title": fixture["title"],
            "origin": {"repo": "acme/api"}, "status": "decided",
            "payload": fixture["payload"], "decision": fixture["decision"], "agent_note": "dont nitpick the docs"
        });
        let md = render(&review, None, Some(&template));
        assert!(md.contains("\n## Proposals\n\n- **#18 reject** `lib/acme/tickets.ex:149` — reversing twice is a no-op with a cost (major)\n  > dont nitpick\n"), "{md}");
        assert!(md.contains("\nUndecided: #19, #20\n"), "{md}");
        assert!(
            !md.contains("## Decisions"),
            "the template replaces the generic body: {md}"
        );

        // a template that fails to compile is reported; one that fails to
        // render falls back to the generic body
        assert!(compile("{% if %}").is_err());
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
        assert!(md.contains("· 1 item\n"), "{md}");
    }
}
