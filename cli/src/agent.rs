//! The coding agent the CLI runs under, from the environment it sets, so a
//! review says who asked without the agent saying so: `requested_by`
//! defaults to it. First the shared conventions, `AI_AGENT` (any agent) and
//! `AGENT` (known names only, since other tools use it too), then each
//! agent's own markers. An agent none of these name asks as `pinrail-cli`.

/// The name a review gives each agent Pinrail knows, as the app shows it.
const KNOWN: [(&str, &str); 6] = [
    ("claude-code", "claude"),
    ("codex", "codex"),
    ("cursor", "cursor"),
    ("gemini-cli", "gemini"),
    ("opencode", "opencode"),
    ("kimi", "kimi"),
];

/// Each agent's own markers, in the order they are checked.
const MARKERS: [(&str, &[&str]); 5] = [
    ("claude-code", &["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT"]),
    ("codex", &["CODEX_THREAD_ID", "CODEX_SANDBOX", "CODEX_CI"]),
    // not CURSOR_TRACE_ID: the editor sets it in a person's terminals too
    ("cursor", &["CURSOR_AGENT"]),
    ("gemini-cli", &["GEMINI_CLI"]),
    ("opencode", &["OPENCODE", "OPENCODE_PID", "OPENCODE_CLIENT"]),
];

pub fn detect() -> Option<String> {
    detect_with(|key| std::env::var(key).ok())
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
/// `Claude`, `gemini` and `gemini-cli` all say which one it is.
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
            detect(&[("GEMINI_CLI", "1")]).as_deref(),
            Some("gemini-cli")
        );
        assert_eq!(detect(&[("OPENCODE", "1")]).as_deref(), Some("opencode"));
        // a person's own terminal in Cursor, and blank values, name nobody
        assert_eq!(
            detect(&[("CURSOR_TRACE_ID", "abc"), ("GEMINI_CLI", " ")]),
            None
        );
    }
}
