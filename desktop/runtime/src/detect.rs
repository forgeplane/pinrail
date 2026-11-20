//! The coding agent the CLI runs under, from the environment it sets, so a
//! review says who asked without the agent saying so: `requested_by`
//! defaults to it. First the shared conventions, `AI_AGENT` (any agent) and
//! `AGENT` (known names only, since other tools use it too), then each
//! agent's own markers. An agent none of these name asks as `pinrail-cli`.
//!
//! An agent that does not set `AI_AGENT`, such as Antigravity CLI, inherits
//! the value of the agent it was started from. When `AI_AGENT` and another
//! agent's markers name two agents, the one whose process runs nearest
//! above the command is detected.

use crate::registry::{AgentSpec, all, find};

/// The order agents' own markers are checked in: Amp before Claude Code,
/// since older versions of Amp also set CLAUDECODE, and Goose last, since
/// its variable's name is generic. The others in the registry's order.
fn by_markers() -> Vec<&'static AgentSpec> {
    let rank = |agent: &AgentSpec| match agent.id {
        "amp" => 0,
        "goose" => 2,
        _ => 1,
    };
    let mut agents: Vec<_> = all().iter().collect();
    agents.sort_by_key(|agent| rank(agent));
    agents
}

pub fn detect() -> Option<String> {
    detect_with(|key| std::env::var(key).ok(), running_nearest)
}

/// The id of the session of the agent the CLI runs under, so a review can
/// be traced back to it; only that agent's own variable counts.
pub fn session() -> Option<String> {
    session_with(|key| std::env::var(key).ok(), running_nearest)
}

pub fn session_with(
    var: impl Fn(&str) -> Option<String>,
    nearest: impl Fn(&[&str]) -> Option<String>,
) -> Option<String> {
    let agent = detect_with(&var, nearest)?;
    let key = find(&agent)?.session_var?;
    var(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The agent the variables `var` name. `nearest` tells which of the
/// agents it is given runs nearest above the command, when two are named.
pub fn detect_with(
    var: impl Fn(&str) -> Option<String>,
    nearest: impl Fn(&[&str]) -> Option<String>,
) -> Option<String> {
    let set = |key: &str| var(key).filter(|v| !v.trim().is_empty());
    if let Some(value) = set("AI_AGENT") {
        let named = known(&value).unwrap_or_else(|| first_word(&value));
        // another agent's markers: one of the two was started from the other
        let mut agents: Vec<&str> = by_markers()
            .into_iter()
            .filter(|agent| agent.id != named && agent.markers.iter().any(|k| set(k).is_some()))
            .map(|agent| agent.id)
            .collect();
        if agents.is_empty() {
            return Some(named);
        }
        agents.insert(0, &named);
        return Some(nearest(&agents).unwrap_or(named));
    }
    if let Some(name) = set("AGENT").and_then(|v| known(&v)) {
        return Some(name);
    }
    by_markers()
        .into_iter()
        .find(|agent| agent.markers.iter().any(|k| set(k).is_some()))
        .map(|agent| agent.id.to_string())
}

/// Of `agents`, the one whose process runs nearest above this one.
fn running_nearest(agents: &[&str]) -> Option<String> {
    let specs: Vec<&AgentSpec> = agents.iter().filter_map(|id| find(id)).collect();
    let binaries: Vec<&str> = specs
        .iter()
        .flat_map(|a| a.binaries.iter().copied())
        .collect();
    let found = crate::process::ancestor(std::process::id(), &binaries)?;
    specs
        .iter()
        .find(|a| a.binaries.contains(&found.name.as_str()))
        .map(|a| a.id.to_string())
}

/// The review's name for an agent a value names: `claude-code_2-1-281_agent`,
/// `Claude`, `agy` and `antigravity` all say which one it is.
fn known(value: &str) -> Option<String> {
    find(&first_word(value)).map(|agent| agent.id.to_string())
}

/// A value's leading name, lowercase: `claude-code_2-1-281_agent` is
/// `claude-code`.
fn first_word(value: &str) -> String {
    value
        .trim()
        .split(|c: char| c == '_' || c == '/' || c == '@' || c.is_whitespace())
        .next()
        .unwrap_or_default()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::detect_with;
    use std::collections::HashMap;

    fn detect(vars: &[(&str, &str)]) -> Option<String> {
        detect_under(vars, None)
    }

    /// The agent detected with `vars`, where `running` is the agent the
    /// process tree finds nearest, if any.
    fn detect_under(vars: &[(&str, &str)], running: Option<&str>) -> Option<String> {
        let env: HashMap<&str, &str> = vars.iter().copied().collect();
        detect_with(
            |k| env.get(k).map(|v| v.to_string()),
            |agents: &[&str]| running.filter(|r| agents.contains(r)).map(str::to_string),
        )
    }

    /// The environment of a shell inside agy that was started from a
    /// Claude Code shell, or of Claude Code started inside agy: both carry
    /// Claude Code's AI_AGENT and agy's conversation.
    const NESTED: &[(&str, &str)] = &[
        ("AI_AGENT", "claude-code_2-1-281_agent"),
        ("CLAUDECODE", "1"),
        ("CLAUDE_CODE_SESSION_ID", "claude-1"),
        ("ANTIGRAVITY_CONVERSATION_ID", "agy-1"),
    ];

    #[test]
    fn of_two_agents_the_environment_names_the_one_that_runs_nearest_is_detected() {
        // agy does not set AI_AGENT: inside it, the value is the one of the
        // Claude Code it was started from
        assert_eq!(
            detect_under(NESTED, Some("antigravity")).as_deref(),
            Some("antigravity")
        );
        // Claude Code started inside agy sets AI_AGENT, and keeps agy's
        // conversation from the shell
        assert_eq!(
            detect_under(NESTED, Some("claude-code")).as_deref(),
            Some("claude-code")
        );
        // neither found among the processes above: AI_AGENT, as before
        assert_eq!(detect_under(NESTED, None).as_deref(), Some("claude-code"));
    }

    #[test]
    fn the_session_is_the_one_of_the_agent_detected() {
        let env: HashMap<&str, &str> = NESTED.iter().copied().collect();
        let session = |running: &'static str| {
            super::session_with(
                |k| env.get(k).map(|v| v.to_string()),
                move |agents: &[&str]| agents.contains(&running).then(|| running.to_string()),
            )
        };
        assert_eq!(session("antigravity").as_deref(), Some("agy-1"));
        assert_eq!(session("claude-code").as_deref(), Some("claude-1"));
    }

    #[test]
    fn the_processes_are_read_only_when_two_agents_are_named() {
        let env: HashMap<&str, &str> = [
            ("AI_AGENT", "claude-code_2-1-281_agent"),
            ("CLAUDECODE", "1"),
        ]
        .into_iter()
        .collect();
        let detected = detect_with(
            |k| env.get(k).map(|v| v.to_string()),
            |_: &[&str]| -> Option<String> { panic!("read the processes") },
        );
        assert_eq!(detected.as_deref(), Some("claude-code"));
    }

    #[test]
    fn an_agent_is_named_by_the_shared_convention_then_by_its_own_markers() {
        assert_eq!(
            detect(&[
                ("AI_AGENT", "claude-code_2-1-281_agent"),
                ("CLAUDECODE", "1")
            ])
            .as_deref(),
            Some("claude-code")
        );
        // any agent AI_AGENT names, known or not
        assert_eq!(detect(&[("AI_AGENT", "goose")]).as_deref(), Some("goose"));
        // AGENT only for a name Pinrail knows: other tools set it too
        assert_eq!(detect(&[("AGENT", "codex")]).as_deref(), Some("codex"));
        assert_eq!(detect(&[("AGENT", "build-agent-7")]), None);
        assert_eq!(
            detect(&[("CLAUDECODE", "1")]).as_deref(),
            Some("claude-code")
        );
        assert_eq!(
            detect(&[("CODEX_SANDBOX", "seatbelt")]).as_deref(),
            Some("codex")
        );
        assert_eq!(detect(&[("CURSOR_AGENT", "1")]).as_deref(), Some("cursor"));
        assert_eq!(
            detect(&[("ANTIGRAVITY_AGENT", "1")]).as_deref(),
            Some("antigravity")
        );
        assert_eq!(
            detect(&[("ANTIGRAVITY_CONVERSATION_ID", "abc")]).as_deref(),
            Some("antigravity")
        );
        assert_eq!(detect(&[("AGENT", "agy")]).as_deref(), Some("antigravity"));
        assert_eq!(detect(&[("OPENCODE", "1")]).as_deref(), Some("opencode"));
        assert_eq!(detect(&[("AI_AGENT", "Grok")]).as_deref(), Some("grok"));
        assert_eq!(detect(&[("AGENT", "grok")]).as_deref(), Some("grok"));
        assert_eq!(detect(&[("GROK_AGENT", "1")]).as_deref(), Some("grok"));
        assert_eq!(
            detect(&[("GROK_SESSION_ID", "abc")]).as_deref(),
            Some("grok")
        );
        assert_eq!(
            detect(&[("PI_CODING_AGENT", "true")]).as_deref(),
            Some("pi")
        );
        assert_eq!(detect(&[("AGENT", "pi")]).as_deref(), Some("pi"));
        assert_eq!(detect(&[("AI_AGENT", "pi")]).as_deref(), Some("pi"));
        assert_eq!(detect(&[("GOOSE_TERMINAL", "1")]).as_deref(), Some("goose"));
        assert_eq!(detect(&[("AGENT", "goose")]).as_deref(), Some("goose"));
        assert_eq!(
            detect(&[("CLAUDECODE", "1"), ("AMP_CURRENT_THREAD_ID", "T-1")]).as_deref(),
            Some("amp")
        );
        assert_eq!(detect(&[("AI_AGENT", "amp")]).as_deref(), Some("amp"));
        assert_eq!(
            detect(&[("CLAUDECODE", "1"), ("AGENT", "amp")]).as_deref(),
            Some("amp")
        );
        assert_eq!(detect(&[("COPILOT_CLI", "1")]).as_deref(), Some("copilot"));
        assert_eq!(
            detect(&[("KIRO_SESSION_ID", "k-1")]).as_deref(),
            Some("kiro")
        );
        assert_eq!(
            detect(&[("AUGMENT_AGENT", "1")]).as_deref(),
            Some("augment")
        );
        assert_eq!(detect(&[("AGENT", "auggie")]).as_deref(), Some("augment"));
        assert_eq!(
            detect(&[("CLINE_ACTIVE", "true")]).as_deref(),
            Some("cline")
        );
        assert_eq!(
            detect(&[("CLINE_WRAPPER_PATH", "/usr/local/bin/cline")]).as_deref(),
            Some("cline")
        );
        assert_eq!(
            detect(&[("HERMES_SESSION_ID", "20261019_1")]).as_deref(),
            Some("hermes-agent")
        );
        // the name Hermes gives itself
        assert_eq!(
            detect(&[("AI_AGENT", "hermes-agent")]).as_deref(),
            Some("hermes-agent")
        );
        assert_eq!(
            detect(&[("AGENT", "hermes")]).as_deref(),
            Some("hermes-agent")
        );
        // the only variable Goose's shell tool sets
        assert_eq!(
            detect(&[("AGENT_SESSION_ID", "20260217_5")]).as_deref(),
            Some("goose")
        );
        // a person's own terminal in Cursor, and blank values, name nobody
        assert_eq!(
            detect(&[("CURSOR_TRACE_ID", "abc"), ("ANTIGRAVITY_AGENT", " ")]),
            None
        );
    }

    fn session(vars: &[(&str, &str)]) -> Option<String> {
        let env: HashMap<&str, &str> = vars.iter().copied().collect();
        super::session_with(|k| env.get(k).map(|v| v.to_string()), |_: &[&str]| None)
    }

    #[test]
    fn cursor_s_session_is_its_conversation() {
        // what Cursor Agent 2026.11.11 sets in a tool shell
        assert_eq!(
            session(&[
                ("CURSOR_AGENT", "1"),
                ("CURSOR_INVOKED_AS", "cursor-agent"),
                ("CURSOR_CONVERSATION_ID", "482560b1-6fc7"),
                ("CURSOR_REQUEST_ID", "3e1e4aa4-b28d"),
            ])
            .as_deref(),
            Some("482560b1-6fc7")
        );
    }
}
