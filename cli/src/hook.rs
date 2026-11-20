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
    // the report's time, read before anything else, so that it is when
    // the hook fired rather than when the command got round to it
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
    if let Some(found) = named_process(&input).or_else(|| agent_process(adapter)) {
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

/// The agent's process its hook names, as Pinrail's extension for Pi does
/// with `pid`, when that process is the one that ran this hook: an input
/// cannot speak for another process.
fn named_process(input: &Value) -> Option<process::Process> {
    let pid = u32::try_from(input.get("pid")?.as_u64()?).ok()?;
    #[cfg(unix)]
    {
        (std::os::unix::process::parent_id() == pid)
            .then(|| process::process(pid))
            .flatten()
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        None
    }
}

/// The agent's own process: from `CLAUDE_PID`, which Claude Code sets for its
/// hooks, or the nearest process above this one that the agent's adapter
/// recognises as its own.
fn agent_process(agent: &dyn pinrail_runtime::Agent) -> Option<process::Process> {
    if agent.spec().id == "claude-code"
        && let Some(pid) = std::env::var("CLAUDE_PID")
            .ok()
            .and_then(|p| p.parse().ok())
    {
        return process::process(pid);
    }
    #[cfg(unix)]
    {
        process::ancestor(std::os::unix::process::parent_id(), &|p| {
            agent.is_own_process(p)
        })
    }
    #[cfg(not(unix))]
    {
        let _ = agent;
        None
    }
}
