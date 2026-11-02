//! The coding agent the CLI runs under, from the environment it sets, so a
//! review says who asked without the agent saying so: `requested_by`
//! defaults to it. First the shared conventions, `AI_AGENT` (any agent) and
//! `AGENT` (known names only, since other tools use it too), then each
//! agent's own markers. An agent none of these name asks as `pinrail-cli`.

/// The name a review gives each agent Pinrail knows, as the app shows it.
const KNOWN: [(&str, &str); 15] = [
    ("claude-code", "claude"),
    ("codex", "codex"),
    ("cursor", "cursor"),
    ("antigravity", "agy"),
    ("opencode", "opencode"),
    ("kimi", "kimi"),
    ("grok", "grok"),
    ("pi", "pi"),
    ("goose", "goose"),
    ("amp", "amp"),
    ("copilot", "copilot"),
    ("kiro", "kiro"),
    ("augment", "auggie"),
    ("cline", "cline"),
    ("hermes-agent", "hermes"),
];

/// Each agent's own markers, in the order they are checked.
const MARKERS: [(&str, &[&str]); 14] = [
    // before Claude Code: older versions of Amp also set CLAUDECODE
    ("amp", &["AMP_CURRENT_THREAD_ID"]),
    ("claude-code", &["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT"]),
    ("codex", &["CODEX_THREAD_ID", "CODEX_SANDBOX", "CODEX_CI"]),
    // not CURSOR_TRACE_ID: the editor sets it in a person's terminals too
    ("cursor", &["CURSOR_AGENT"]),
    (
        "antigravity",
        &["ANTIGRAVITY_AGENT", "ANTIGRAVITY_CONVERSATION_ID"],
    ),
    ("opencode", &["OPENCODE", "OPENCODE_PID", "OPENCODE_CLIENT"]),
    ("grok", &["GROK_AGENT", "GROK_SESSION_ID"]),
    ("pi", &["PI_CODING_AGENT"]),
    ("copilot", &["COPILOT_CLI"]),
    ("kiro", &["KIRO_SESSION_ID"]),
    ("augment", &["AUGMENT_AGENT"]),
    // CLINE_ACTIVE on the terminals of Cline's VS Code extension, and
    // CLINE_WRAPPER_PATH from the launcher of its CLI
    ("cline", &["CLINE_ACTIVE", "CLINE_WRAPPER_PATH"]),
    ("hermes-agent", &["HERMES_SESSION_ID"]),
    // Goose's shell tool sets only AGENT_SESSION_ID; GOOSE_TERMINAL and
    // AGENT=goose are set for its other commands. Checked last, since the
    // name is generic.
    ("goose", &["GOOSE_TERMINAL", "AGENT_SESSION_ID"]),
];

/// The variable each agent keeps its session's id in.
const SESSIONS: [(&str, &str); 11] = [
    ("claude-code", "CLAUDE_CODE_SESSION_ID"),
    ("codex", "CODEX_SESSION_ID"),
    ("opencode", "OPENCODE_SESSION_ID"),
    ("grok", "GROK_SESSION_ID"),
    ("antigravity", "ANTIGRAVITY_CONVERSATION_ID"),
    ("pi", "PI_SESSION_ID"),
    // a generic name, read only when Goose is the detected agent
    ("goose", "AGENT_SESSION_ID"),
    ("amp", "AMP_CURRENT_THREAD_ID"),
    ("copilot", "COPILOT_AGENT_SESSION_ID"),
    ("kiro", "KIRO_SESSION_ID"),
    ("hermes-agent", "HERMES_SESSION_ID"),
];

pub fn detect() -> Option<String> {
    detect_with(|key| std::env::var(key).ok())
}

/// The id of the session of the agent the CLI runs under, so a review can
/// be traced back to it; only that agent's own variable counts.
pub fn session() -> Option<String> {
    session_with(|key| std::env::var(key).ok())
}

pub fn session_with(var: impl Fn(&str) -> Option<String>) -> Option<String> {
    let agent = detect_with(&var)?;
    let (_, key) = SESSIONS.iter().find(|(name, _)| *name == agent)?;
    var(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub fn detect_with(var: impl Fn(&str) -> Option<String>) -> Option<String> {
    let set = |key: &str| var(key).filter(|v| !v.trim().is_empty());
    if let Some(value) = set("AI_AGENT") {
        return Some(known(&value).unwrap_or_else(|| first_word(&value)));
    }
    if let Some(name) = set("AGENT").and_then(|v| known(&v)) {
        return Some(name);
    }
    MARKERS
        .iter()
        .find(|(_, keys)| keys.iter().any(|k| set(k).is_some()))
        .map(|(name, _)| name.to_string())
}

/// The review's name for an agent a value names: `claude-code_2-1-281_agent`,
/// `Claude`, `agy` and `antigravity` all say which one it is.
fn known(value: &str) -> Option<String> {
    let word = first_word(value);
    KNOWN
        .iter()
        .find(|(name, short)| word == *name || word == *short)
        .map(|(name, _)| name.to_string())
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
        let env: HashMap<&str, &str> = vars.iter().copied().collect();
        detect_with(|k| env.get(k).map(|v| v.to_string()))
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
}
