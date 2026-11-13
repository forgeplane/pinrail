//! `pinrail hook <agent> <event>`: what an agent's hook reports about its
//! session, posted to the app. The hooks Pinrail installs in an agent run it
//! with the hook's JSON on stdin. It must never slow the agent down or get
//! in its way, so it prints nothing, always exits 0, never starts the
//! server, and gives up after a second in all.

use std::io::Read;
use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
use serde_json::{Value, json};

use pinrail_runtime::hooks::HookEvent;
use pinrail_runtime::process;

/// The longest the command takes, from start to exit.
const BUDGET: Duration = Duration::from_secs(1);

#[derive(clap::Args, Debug)]
pub struct HookArgs {
    /// The agent the hook belongs to, such as claude-code or codex
    pub agent: String,
    /// Pinrail's name for the hook's event, such as prompt or stop, as the
    /// installed hook gives it
    pub event: String,
}

/// Reads the hook's input, and posts what it reports, if anything. Every
/// failure ends quietly: the agent goes on either way.
pub fn run(args: &HookArgs, url: Option<&str>) {
    let started = Instant::now();
    // the report's time, read before anything else (spec 5.5)
    let at = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true);
    // to the end, whatever its size, so the agent never gets a write error
    let mut input = Vec::new();
    if std::io::stdin().read_to_end(&mut input).is_err() {
        return;
    }
    let Some((agent, session, body)) = report(args, &input, &at) else {
        return;
    };
    let Ok(base) = crate::server::resolve_url(url, false) else {
        return;
    };
    let left = BUDGET.saturating_sub(started.elapsed());
    if left.is_zero() {
        return;
    }
    let token = std::env::var("PINRAIL_RUN_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());
    crate::api::post_report(&base, agent, &session, &body, token.as_deref(), left);
}

/// The agent's id, the session and the body of the report the hook makes,
/// or `None` when it reports nothing.
fn report(args: &HookArgs, input: &[u8], at: &str) -> Option<(&'static str, String, Value)> {
    let input: Value = serde_json::from_slice(input).ok()?;
    let adapter = pinrail_runtime::adapter(&args.agent)?;
    let report = adapter.parse_hook(&args.event, &input, &|key| std::env::var(key).ok())?;
    let spec = adapter.spec();
    let mut body = report.to_body();
    body["at"] = json!(at);
    if let Some(found) = agent_process(spec.id, spec.binaries) {
        body["process"] = json!({"pid": found.pid, "started": found.started});
    }
    // where the session works: the first report creates the session, and a
    // start or a prompt is the first an agent sends; git is asked only then
    if let Some(cwd) = &report.detail.cwd {
        body["origin"] = json!({"folder": cwd});
    }
    if matches!(report.event, HookEvent::Started { .. } | HookEvent::Prompt) {
        crate::origin::fill_quietly(&mut body);
    }
    Some((spec.id, report.session, body))
}

/// The agent's own process: from `CLAUDE_PID`, which Claude Code sets for its
/// hooks, or the nearest process above this one with one of the agent's
/// names.
fn agent_process(agent: &str, binaries: &[&str]) -> Option<process::Process> {
    if agent == "claude-code"
        && let Some(pid) = std::env::var("CLAUDE_PID")
            .ok()
            .and_then(|p| p.parse().ok())
    {
        return process::process(pid);
    }
    #[cfg(unix)]
    {
        process::ancestor(std::os::unix::process::parent_id(), binaries)
    }
    #[cfg(not(unix))]
    {
        let _ = binaries;
        None
    }
}
