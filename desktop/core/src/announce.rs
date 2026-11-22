//! Which notices raise a system notification, decided in the core so that
//! the desktop, and later the phone, follow one rule. A notification is
//! announced only when its event reached the inbox, its reason is one the
//! person announces, and notifications are on: not paused, not in the quiet
//! hours, and not muted for the review's plugin.

use chrono::{DateTime, Local, NaiveTime, Utc};
use serde_json::{Value, json};

/// The setting under `/notifications/announce` that a reason follows, and
/// whether it is on when the person never set it.
fn announced_by(reason: &str) -> Option<(&'static str, bool)> {
    Some(match reason {
        "review_requested" | "new_round" => ("review_requested", true),
        "question_asked" => ("question_asked", true),
        "permission_requested" | "input_requested" => ("agent_waits", false),
        "agent_finished" | "agent_failed" | "agent_stopped" => ("agent_finished", false),
        "item_commented" => ("item_commented", false),
        _ => return None,
    })
}

/// What the desktop shows for a notification that `event` changed, with
/// the `/notifications` settings, at `now`: its title, its body, whether it
/// sounds, and the tag that closes it later. `None` when it is not
/// announced.
pub fn announce(
    settings: &Value,
    notification: &Value,
    event: i64,
    now: DateTime<Local>,
) -> Option<Value> {
    // the event reached the inbox: it is the notification's latest, unread
    if notification["last_event"].as_i64() != Some(event)
        || notification["unread"].as_i64().unwrap_or(0) == 0
        || notification["done"] == true
    {
        return None;
    }
    let reason = notification["reason"].as_str()?;
    let (key, default) = announced_by(reason)?;
    if !settings["announce"][key].as_bool().unwrap_or(default) || !on(settings, now) {
        return None;
    }
    let subject = &notification["subject"];
    if let Some(plugin) = subject["review"]["plugin"].as_str()
        && settings["muted_plugins"]
            .as_array()
            .is_some_and(|muted| muted.iter().any(|m| m == plugin))
    {
        return None;
    }
    let (title, body) = words(reason, notification);
    Some(json!({
        "title": title,
        "body": body,
        "sound": settings["sound"].as_bool().unwrap_or(true),
        "tag": tag(notification),
    }))
}

/// The tag a notification is shown and closed by: a review's id, as before,
/// or the notification's own.
pub fn tag(notification: &Value) -> String {
    match notification["subject"]["review"]["id"].as_str() {
        Some(id) => id.to_string(),
        None => format!("n-{}", notification["id"]),
    }
}

/// Whether notifications are on at `now`: enabled, not paused, and outside
/// the quiet hours, which may run across midnight.
fn on(settings: &Value, now: DateTime<Local>) -> bool {
    let paused = settings["paused_until"]
        .as_str()
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .is_some_and(|t| t.with_timezone(&Utc) > now.with_timezone(&Utc));
    let clock = |key: &str| {
        settings["quiet_hours"][key]
            .as_str()
            .and_then(|t| NaiveTime::parse_from_str(t, "%H:%M").ok())
    };
    let quiet = clock("from").zip(clock("to")).is_some_and(|(from, to)| {
        let t = now.time();
        if from <= to {
            from <= t && t < to
        } else {
            t >= from || t < to
        }
    });
    settings["enabled"].as_bool().unwrap_or(true) && !paused && !quiet
}

/// The title and the body of a notification, by its reason.
fn words(reason: &str, n: &Value) -> (String, String) {
    let subject = &n["subject"];
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let summary = n["summary"].as_str();
    match subject["kind"].as_str() {
        Some("review") => {
            let review = &subject["review"];
            let title = review["title"]
                .as_str()
                .unwrap_or("A review is waiting")
                .to_string();
            let plugin = text(&review["plugin"]);
            let by = review["requested_by"].as_str();
            let mut lines = Vec::new();
            if let Some(counts) = counts(&review["summary"]) {
                lines.push(counts);
            }
            lines.push(match (reason, by, review["session_name"].as_str()) {
                ("question_asked", Some(by), _) => format!("{by} asks you"),
                (_, Some(by), Some(session)) => {
                    format!("{plugin} · requested by {by} in {session}")
                }
                (_, Some(by), None) => format!("{plugin} · requested by {by}"),
                _ => plugin,
            });
            (title, lines.join("\n"))
        }
        Some("session") => {
            let session = &subject["session"];
            let agent = text(&session["agent"]);
            let title = ["name", "title", "generated_name"]
                .iter()
                .find_map(|k| session[*k].as_str().filter(|s| !s.is_empty()))
                .unwrap_or("A session")
                .to_string();
            let what = match reason {
                "permission_requested" => format!("{agent} waits for your permission"),
                "input_requested" => format!("{agent} waits for your answer"),
                "agent_failed" => format!("{agent}'s turn failed"),
                "agent_stopped" => format!("{agent} stopped"),
                _ => format!("{agent} finished"),
            };
            let detail = n["new"]["reply"].as_str().or(summary);
            (
                title,
                detail.map_or(what.clone(), |d| format!("{what} · {d}")),
            )
        }
        _ => {
            let item = &subject["item"];
            let title = format!("{} {}", text(&item["key"]), text(&item["title"]));
            let who = n["actor"]["name"].as_str().unwrap_or("An agent");
            let body = match summary.or(n["new"]["comment"].as_str()) {
                Some(comment) => format!("{who} commented: {comment}"),
                None => format!("{who} commented"),
            };
            (title, body)
        }
    }
}

/// A review's counts as one line, such as "1 blocker, 2 major".
fn counts(summary: &Value) -> Option<String> {
    let parts: Vec<String> = summary["counts"]
        .as_array()?
        .iter()
        .filter_map(|c| Some(format!("{} {}", c["count"].as_u64()?, c["label"].as_str()?)))
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}
