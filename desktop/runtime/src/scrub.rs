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

/// What launchd gives the app that started the agent, such as an app
/// opened from Finder. With `XPC_FLAGS` set so, Claude Code cannot read
/// its login from the keychain.
const APP_VARIABLES: &[&str] = &["XPC_FLAGS", "XPC_SERVICE_NAME", "__CFBundleIdentifier"];

/// Pinrail's own, which the new run sets again.
const PINRAIL_VARIABLES: &[&str] = &[
    "PINRAIL_RUN_ID",
    "PINRAIL_RUN_TOKEN",
    "PINRAIL_REQUESTED_BY",
];

/// Whether a variable is removed before an agent starts: every agent's
/// markers and session variable, terminal handles, the launching app's
/// variables, and Pinrail's run variables.
/// On Windows a name is one in any case, as Windows gives it to the
/// agent under any spelling.
pub fn is_scrubbed(name: &str) -> bool {
    is_scrubbed_on(name, cfg!(windows))
}

fn is_scrubbed_on(name: &str, windows: bool) -> bool {
    let is = |known: &str| {
        if windows {
            known.eq_ignore_ascii_case(name)
        } else {
            known == name
        }
    };
    let listed = |list: &[&str]| list.iter().any(|known| is(known));
    listed(AGENT_VARIABLES)
        || listed(TERMINAL_HANDLES)
        || listed(APP_VARIABLES)
        || listed(PINRAIL_VARIABLES)
        || all()
            .iter()
            .any(|agent| listed(agent.markers) || agent.session_var.is_some_and(is))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_windows_a_variable_is_scrubbed_in_any_case() {
        for name in ["ClaudeCode", "pinrail_run_token", "Codex_Thread_Id", "tmux"] {
            assert!(is_scrubbed_on(name, true), "{name}");
            assert!(!is_scrubbed_on(name, false), "{name}");
        }
        assert!(!is_scrubbed_on("Path", true));
    }

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
