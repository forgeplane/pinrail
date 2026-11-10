//! Crash reports: a panic in the core or the app shell, or an uncaught
//! error in the window, kept on disk with paths, quoted text, links,
//! email addresses and secrets removed, and sent at the next start.
//!
//! One report waits at a time: the first since the last was sent, which is
//! usually the cause of those after it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{Context, Sent, Telemetry};

/// The file in the data directory that holds the report waiting to be sent.
pub(super) const FILE: &str = "crash-report.json";
/// The longest message and stack the endpoint takes, in characters.
const MAX_MESSAGE: usize = 2_000;
const MAX_STACK: usize = 32_000;

/// Where a crash happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Core,
    Shell,
    Ui,
}

/// A crash, scrubbed, as it waits on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub kind: Kind,
    pub at: DateTime<Utc>,
    pub message: String,
    pub stack: String,
}

/// A report as the endpoint takes it.
#[derive(Debug, Serialize)]
struct Body<'a> {
    install: Option<String>,
    context: Context,
    kind: Kind,
    at: DateTime<Utc>,
    message: &'a str,
    stack: &'a str,
}

impl Report {
    /// A report of `message` and `stack`, scrubbed and cut to the lengths
    /// the endpoint takes.
    pub fn new(kind: Kind, message: &str, stack: &str) -> Report {
        let fit = |text: &str, max: usize| {
            let text = scrub(text);
            let text = if text.trim().is_empty() {
                "…".to_string()
            } else {
                text
            };
            text.chars().take(max).collect()
        };
        Report {
            kind,
            at: Utc::now(),
            message: fit(message, MAX_MESSAGE),
            stack: fit(stack, MAX_STACK),
        }
    }
}

/// The text with what could identify a person or their work removed: a
/// path becomes its file name, and quoted text, a link, an email address
/// and anything that looks like a key or a token become `…`. Control
/// characters other than tabs and line breaks are dropped.
pub fn scrub(text: &str) -> String {
    let unquoted = unquote(text);
    let mut out = String::with_capacity(unquoted.len());
    let mut word = String::new();
    for c in unquoted.chars() {
        if c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',') {
            out.push_str(&scrub_word(&word));
            word.clear();
            if !c.is_control() || matches!(c, '\n' | '\t') {
                out.push(if c == '\r' { '\n' } else { c });
            }
        } else if !c.is_control() {
            word.push(c);
        }
    }
    out.push_str(&scrub_word(&word));
    out
}

/// The text with whatever is in quotes, on one line, replaced by `…`.
fn unquote(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut open: Option<char> = None;
    for c in text.chars() {
        match open {
            Some(q) if c == q => {
                out.push('…');
                out.push(c);
                open = None;
            }
            Some(_) if c == '\n' => {
                // a quote never closed on its line was an apostrophe
                open = None;
                out.push(c);
            }
            Some(_) => {}
            None if matches!(c, '"' | '`') => {
                out.push(c);
                open = Some(c);
            }
            None => out.push(c),
        }
    }
    if open.is_some() {
        out.push('…');
    }
    out
}

fn scrub_word(word: &str) -> String {
    if word.is_empty() {
        return String::new();
    }
    let lower = word.to_ascii_lowercase();
    if lower.contains("://") || lower.starts_with("www.") {
        return "…".to_string();
    }
    if let Some((user, host)) = word.split_once('@')
        && !user.is_empty()
        && host.contains('.')
    {
        return "…".to_string();
    }
    if word.contains('/') || word.contains('\\') {
        // a path: its last part, such as `telemetry.rs:12:5`
        let name = word.rsplit(['/', '\\']).next().unwrap_or_default();
        return if name.is_empty() {
            "…".to_string()
        } else {
            scrub_word(name)
        };
    }
    let secretish = word.len() >= 24
        && !word.contains("::")
        && word.bytes().any(|b| b.is_ascii_digit())
        && word.bytes().any(|b| b.is_ascii_alphabetic())
        && word
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_+=.".contains(&b));
    if secretish {
        return "…".to_string();
    }
    word.to_string()
}

impl Telemetry {
    /// Keeps `report` to send at the next start, while crash reports are
    /// on and no other report waits.
    pub fn keep_crash(&self, report: &Report) {
        if !self.crashes_allowed() {
            return;
        }
        let path = self.crash_path();
        if path.exists() {
            return;
        }
        if let Ok(json) = serde_json::to_vec(report) {
            let _ = std::fs::write(path, json);
        }
    }

    /// The report waiting to be sent, if any.
    pub fn waiting_crash(&self) -> Option<Report> {
        let bytes = std::fs::read(self.crash_path()).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub(super) fn crash_path(&self) -> PathBuf {
        self.data_dir.join(FILE)
    }

    /// Sends the report that waits to `url`, then deletes it, unless the
    /// send failed, which leaves it for the next start. Blocks: call it
    /// off the async runtime.
    pub fn send_crash(&self, url: &str, debug: bool) -> Option<Sent> {
        if !self.crashes_on() {
            return None;
        }
        let report = self.waiting_crash()?;
        let body = Body {
            // the install id only while usage data is on too
            install: self.usage_on().then(|| self.install_id()).flatten(),
            context: Context::of_this_system(),
            kind: report.kind,
            at: report.at,
            message: &report.message,
            stack: &report.stack,
        };
        let sent = super::send::post(url, debug, &body);
        if sent != Sent::Failed {
            let _ = std::fs::remove_file(self.crash_path());
        }
        Some(sent)
    }
}

/// Keeps a report of every panic in this process from now on, as a crash
/// of the core or of the app shell by where it happened, and then runs the
/// hook that was there before, which prints it. Only the first call sets
/// the hook.
pub fn catch_panics(telemetry: Arc<Telemetry>) {
    static SET: OnceLock<()> = OnceLock::new();
    if SET.set(()).is_err() {
        return;
    }
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let place = info.location();
        let kind = match place {
            Some(place) if place.file().contains("src-tauri") => Kind::Shell,
            _ => Kind::Core,
        };
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic with no message".to_string());
        let message = match place {
            Some(place) => format!("{message} at {}:{}", place.file(), place.line()),
            None => message,
        };
        let stack = std::backtrace::Backtrace::force_capture().to_string();
        telemetry.keep_crash(&Report::new(kind, &message, &stack));
        before(info);
    }));
}

/// The crashes address that goes with the events address `url`: its last
/// part, `events`, replaced with `crashes`.
pub fn crashes_url(url: &str) -> String {
    match url.trim_end_matches('/').rsplit_once('/') {
        Some((base, _)) => format!("{base}/crashes"),
        None => url.to_string(),
    }
}

/// Deletes the report that waits, for when crash reports are turned off.
pub(super) fn forget(data_dir: &Path) {
    let _ = std::fs::remove_file(data_dir.join(FILE));
}

#[cfg(test)]
mod tests {
    use super::super::send::tests::{Got, endpoint_at};
    use super::*;
    use crate::{Config, Pinrail};
    use axum::http::StatusCode;
    use serde_json::{Value, json};

    fn app() -> (tempfile::TempDir, Pinrail) {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::new(dir.path(), 0);
        config.telemetry_env = false;
        let app = Pinrail::open(config).unwrap();
        app.settings()
            .change(&json!({"telemetry": {"notice_shown": "2026-11-10T09:00:00Z"}}))
            .unwrap();
        (dir, app)
    }

    #[test]
    fn scrubbing_removes_paths_quotes_links_addresses_and_secrets() {
        for (text, scrubbed) in [
            (
                "called `Result::unwrap()` on an `Err` value at /Users/ana/code/acme/src/payroll.rs:12:5",
                "called `…` on an `…` value at payroll.rs:12:5",
            ),
            (
                r#"no such file "C:\Users\ana\Acme Payroll.md" (os error 2)"#,
                r#"no such file "…" (os error 2)"#,
            ),
            (
                "fetch https://acme.example/api?token=x failed for ana@acme.example",
                "fetch … failed for …",
            ),
            (
                "key sk_live_51HxA9fK2eZvKYlo2C8x7 rejected",
                "key … rejected",
            ),
            (
                "   0: pinrail_core::telemetry::Telemetry::record_at\n             at ./core/src/telemetry.rs:300:9",
                "   0: pinrail_core::telemetry::Telemetry::record_at\n             at telemetry.rs:300:9",
            ),
            ("thread 'main' panicked\u{7}", "thread 'main' panicked"),
        ] {
            assert_eq!(scrub(text), scrubbed, "{text}");
        }
    }

    #[test]
    fn a_report_is_cut_to_the_lengths_the_endpoint_takes() {
        let report = Report::new(Kind::Ui, &"m".repeat(5_000), &"s ".repeat(40_000));
        assert_eq!(report.message.chars().count(), MAX_MESSAGE);
        assert_eq!(report.stack.chars().count(), MAX_STACK);
        assert_eq!(Report::new(Kind::Ui, "", "").message, "…");
    }

    #[test]
    fn the_first_report_waits_while_crash_reports_are_on() {
        let (_dir, app) = app();
        let t = app.telemetry();
        let first = Report::new(Kind::Core, "first", "stack");
        t.keep_crash(&first);
        t.keep_crash(&Report::new(Kind::Ui, "second", "stack"));
        assert_eq!(t.waiting_crash(), Some(first.clone()));
        assert_eq!(t.status()["crash"]["message"], "first");

        // off: deleted, and nothing kept
        app.settings()
            .change(&json!({"telemetry": {"crashes": false}}))
            .unwrap();
        assert_eq!(t.waiting_crash(), None);
        t.keep_crash(&first);
        assert_eq!(t.waiting_crash(), None);
    }

    #[test]
    fn nothing_is_kept_before_the_notice_was_shown() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::new(dir.path(), 0);
        config.telemetry_env = false;
        let app = Pinrail::open(config).unwrap();
        app.telemetry()
            .keep_crash(&Report::new(Kind::Core, "boom", "stack"));
        assert_eq!(app.telemetry().waiting_crash(), None);
    }

    #[test]
    fn the_waiting_report_is_sent_then_deleted_unless_the_send_failed() {
        let (_dir, app) = app();
        let t = app.telemetry();
        let report = Report::new(Kind::Shell, "boom", "0: main");
        t.keep_crash(&report);

        let failing = endpoint_at(
            "/v1/crashes",
            StatusCode::SERVICE_UNAVAILABLE,
            Got::default(),
        );
        assert_eq!(t.send_crash(&failing, false), Some(Sent::Failed));
        assert_eq!(t.waiting_crash(), Some(report.clone()));

        let got = Got::default();
        let url = endpoint_at("/v1/crashes", StatusCode::NO_CONTENT, got.clone());
        assert_eq!(t.send_crash(&url, false), Some(Sent::Stored));
        assert_eq!(t.waiting_crash(), None);
        assert_eq!(t.send_crash(&url, false), None);

        let body: Value = serde_json::from_str(&got.lock().unwrap()[0].1).unwrap();
        assert_eq!(body["install"], t.install_id().unwrap());
        assert_eq!(body["kind"], "shell");
        assert_eq!(body["message"], "boom");
        assert_eq!(body["stack"], "0: main");
        assert_eq!(body["context"]["version"], crate::VERSION);
        let mut keys: Vec<_> = body.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            ["at", "context", "install", "kind", "message", "stack"]
        );

        // with usage data off, the report goes without the install id
        app.settings()
            .change(&json!({"telemetry": {"usage": false}}))
            .unwrap();
        t.keep_crash(&report);
        let got = Got::default();
        let url = endpoint_at("/v1/crashes", StatusCode::NO_CONTENT, got.clone());
        assert_eq!(t.send_crash(&url, false), Some(Sent::Stored));
        let body: Value = serde_json::from_str(&got.lock().unwrap()[0].1).unwrap();
        assert_eq!(body["install"], Value::Null);
    }

    /// The deployed endpoint stores a report the app makes. It sends one
    /// test report to the real endpoint, so it runs only when asked:
    /// `cargo test -p pinrail-core -- --ignored deployed`.
    #[test]
    #[ignore]
    fn the_deployed_endpoint_stores_a_crash_report() {
        let (_dir, app) = app();
        let t = app.telemetry();
        t.keep_crash(&Report::new(Kind::Core, "a test report", "0: main"));
        let url = crashes_url(super::super::ENDPOINT);
        assert_eq!(t.send_crash(&url, false), Some(Sent::Stored));
    }

    #[test]
    fn the_crashes_address_is_beside_the_events_one() {
        assert_eq!(
            crashes_url("https://api.pinrail.dev/v1/events"),
            "https://api.pinrail.dev/v1/crashes"
        );
        assert_eq!(
            crashes_url("http://127.0.0.1:8787/v1/events/"),
            "http://127.0.0.1:8787/v1/crashes"
        );
    }
}
