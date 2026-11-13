//! The variables removed from an agent's environment before Pinrail starts
//! it, so that the new agent does not look like the child of another
//! agent's session or of another terminal. The CLI and the terminal host
//! read the same list.

use crate::registry::all;

/// Ids and markers that agents set beyond the registry's own markers.
const AGENT_VARIABLES: &[&str] = &[
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CODEX_SESSION_ID",
    "AI_AGENT",
    "AGENT",
];

/// Handles of the terminal or multiplexer the variables came from.
const TERMINAL_HANDLES: &[&str] = &[
    "TMUX",
    "TMUX_PANE",
    "STY",
    "ITERM_SESSION_ID",
    "KITTY_WINDOW_ID",
    "WEZTERM_PANE",
    "ALACRITTY_WINDOW_ID",
    "GHOSTTY_RESOURCES_DIR",
    "VSCODE_INJECTION",
    "TERM_SESSION_ID",
];

/// Pinrail's own, which the new run sets again.
const PINRAIL_VARIABLES: &[&str] = &[
    "PINRAIL_RUN_ID",
    "PINRAIL_RUN_TOKEN",
    "PINRAIL_REQUESTED_BY",
];

/// Whether a variable is removed before an agent starts: every agent's
/// markers and session variable, terminal handles, and Pinrail's run
/// variables.
pub fn is_scrubbed(name: &str) -> bool {
    AGENT_VARIABLES.contains(&name)
        || TERMINAL_HANDLES.contains(&name)
        || PINRAIL_VARIABLES.contains(&name)
        || all()
            .iter()
            .any(|agent| agent.markers.contains(&name) || agent.session_var == Some(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_markers_terminal_handles_and_run_variables_are_scrubbed() {
        for name in [
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDE_CODE_SESSION_ID",
            "CODEX_THREAD_ID",
            "CODEX_SANDBOX",
            "AI_AGENT",
            "TMUX",
            "TERM_SESSION_ID",
            "PINRAIL_RUN_TOKEN",
        ] {
            assert!(is_scrubbed(name), "{name}");
        }
        for name in ["PATH", "HOME", "TERM", "PINRAIL_URL", "LANG"] {
            assert!(!is_scrubbed(name), "{name}");
        }
    }

    #[test]
    fn every_agent_s_markers_are_scrubbed() {
        for agent in all() {
            for marker in agent.markers {
                assert!(is_scrubbed(marker), "{} {marker}", agent.id);
            }
        }
    }
}
