//! Every coding agent Pinrail knows, as data: its names, the variables that
//! show it runs, the variable that holds its session, and where it keeps
//! its configuration and skills. Detection and skill installation read
//! this one list.

/// What an agent is, as data.
#[derive(Debug)]
pub struct AgentSpec {
    /// The stored id, as `requested_by` and `sessions.agent` hold it.
    pub id: &'static str,
    /// Other names for it, such as its binary, that `AI_AGENT` and `AGENT`
    /// may give.
    pub aliases: &'static [&'static str],
    /// Its name, as the app shows it.
    pub label: &'static str,
    /// Environment variables that mean this agent runs the command.
    pub markers: &'static [&'static str],
    /// The variable holding its session's id, if it sets one.
    pub session_var: Option<&'static str>,
    /// Where it keeps its configuration and skills, if Pinrail installs its
    /// skill there.
    pub folders: Option<Folders>,
}

/// An agent's folders, under the home folder.
#[derive(Debug)]
pub struct Folders {
    /// Its configuration folder, which shows the agent is installed.
    pub config: &'static str,
    /// Where it keeps global skills.
    pub skills: &'static str,
    /// The ids of other agents whose skill folders it also reads. The skill
    /// is still installed in its own folder.
    pub also_reads: &'static [&'static str],
}

/// Every agent, in the order Settings lists them.
pub fn all() -> &'static [AgentSpec] {
    AGENTS
}

/// The agent an id or an alias names.
pub fn find(name: &str) -> Option<&'static AgentSpec> {
    AGENTS
        .iter()
        .find(|a| a.id == name || a.aliases.contains(&name))
}

const AGENTS: &[AgentSpec] = &[
    AgentSpec {
        id: "claude-code",
        aliases: &["claude"],
        label: "Claude Code",
        markers: &["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT"],
        session_var: Some("CLAUDE_CODE_SESSION_ID"),
        folders: Some(Folders {
            config: ".claude",
            skills: ".claude/skills",
            also_reads: &[],
        }),
    },
    AgentSpec {
        id: "codex",
        aliases: &[],
        label: "Codex",
        markers: &["CODEX_THREAD_ID", "CODEX_SANDBOX", "CODEX_CI"],
        session_var: Some("CODEX_SESSION_ID"),
        folders: Some(Folders {
            config: ".codex",
            skills: ".codex/skills",
            also_reads: &[],
        }),
    },
    AgentSpec {
        id: "cursor",
        aliases: &[],
        label: "Cursor",
        // not CURSOR_TRACE_ID: the editor sets it in a person's terminals too
        markers: &["CURSOR_AGENT"],
        session_var: Some("CURSOR_CONVERSATION_ID"),
        folders: Some(Folders {
            config: ".cursor",
            skills: ".cursor/skills",
            also_reads: &[],
        }),
    },
    AgentSpec {
        id: "antigravity",
        aliases: &["agy"],
        label: "Antigravity CLI",
        markers: &["ANTIGRAVITY_AGENT", "ANTIGRAVITY_CONVERSATION_ID"],
        session_var: Some("ANTIGRAVITY_CONVERSATION_ID"),
        folders: Some(Folders {
            config: ".gemini",
            skills: ".gemini/config/skills",
            also_reads: &[],
        }),
    },
    AgentSpec {
        id: "opencode",
        aliases: &[],
        label: "OpenCode",
        markers: &["OPENCODE", "OPENCODE_PID", "OPENCODE_CLIENT"],
        session_var: Some("OPENCODE_SESSION_ID"),
        folders: Some(Folders {
            config: ".config/opencode",
            skills: ".config/opencode/skills",
            // OpenCode also reads ~/.claude/skills. It loads one skill per
            // name, so installing in both folders does not duplicate it.
            also_reads: &["claude-code"],
        }),
    },
    AgentSpec {
        id: "grok",
        aliases: &[],
        label: "Grok CLI",
        markers: &["GROK_AGENT", "GROK_SESSION_ID"],
        session_var: Some("GROK_SESSION_ID"),
        folders: Some(Folders {
            config: ".grok",
            skills: ".grok/skills",
            // Grok also reads ~/.claude/skills and ~/.cursor/skills, unless
            // the person turns that off, and loads one skill per name
            also_reads: &["claude-code", "cursor"],
        }),
    },
    AgentSpec {
        id: "pi",
        aliases: &[],
        label: "Pi",
        markers: &["PI_CODING_AGENT"],
        session_var: Some("PI_SESSION_ID"),
        folders: Some(Folders {
            config: ".pi/agent",
            skills: ".pi/agent/skills",
            also_reads: &[],
        }),
    },
    AgentSpec {
        id: "goose",
        aliases: &[],
        label: "Goose",
        // Goose's shell tool sets only AGENT_SESSION_ID; GOOSE_TERMINAL and
        // AGENT=goose are set for its other commands. Detection checks
        // Goose last, since the name is generic.
        markers: &["GOOSE_TERMINAL", "AGENT_SESSION_ID"],
        // a generic name, read only when Goose is the detected agent
        session_var: Some("AGENT_SESSION_ID"),
        folders: Some(Folders {
            config: ".config/goose",
            // not ~/.agents/skills, which other agents read too
            skills: ".config/goose/skills",
            // Goose also reads ~/.claude/skills and loads the first skill of
            // a name it finds, so installing in both folders does not
            // duplicate it
            also_reads: &["claude-code"],
        }),
    },
    AgentSpec {
        id: "amp",
        aliases: &[],
        label: "Amp",
        // detection checks Amp before Claude Code: older versions of Amp
        // also set CLAUDECODE
        markers: &["AMP_CURRENT_THREAD_ID"],
        session_var: Some("AMP_CURRENT_THREAD_ID"),
        folders: Some(Folders {
            config: ".config/amp",
            // not ~/.config/agents/skills, which other agents read too
            skills: ".config/amp/skills",
            // Amp also reads ~/.claude/skills and loads the first skill of a
            // name it finds, so installing in both folders does not
            // duplicate it
            also_reads: &["claude-code"],
        }),
    },
    AgentSpec {
        id: "copilot",
        aliases: &[],
        label: "Copilot CLI",
        markers: &["COPILOT_CLI"],
        session_var: Some("COPILOT_AGENT_SESSION_ID"),
        folders: Some(Folders {
            config: ".copilot",
            skills: ".copilot/skills",
            also_reads: &[],
        }),
    },
    AgentSpec {
        id: "kiro",
        aliases: &[],
        label: "Kiro CLI",
        markers: &["KIRO_SESSION_ID"],
        session_var: Some("KIRO_SESSION_ID"),
        folders: Some(Folders {
            config: ".kiro",
            skills: ".kiro/skills",
            also_reads: &[],
        }),
    },
    AgentSpec {
        id: "augment",
        aliases: &["auggie"],
        label: "Augment CLI",
        markers: &["AUGMENT_AGENT"],
        session_var: None,
        folders: Some(Folders {
            config: ".augment",
            skills: ".augment/skills",
            // Augment also reads ~/.claude/skills and loads the first skill
            // of a name it finds, so installing in both folders does not
            // duplicate it
            also_reads: &["claude-code"],
        }),
    },
    AgentSpec {
        id: "cline",
        aliases: &[],
        label: "Cline",
        // CLINE_ACTIVE on the terminals of Cline's VS Code extension, and
        // CLINE_WRAPPER_PATH from the launcher of its CLI
        markers: &["CLINE_ACTIVE", "CLINE_WRAPPER_PATH"],
        session_var: None,
        folders: Some(Folders {
            config: ".cline",
            skills: ".cline/skills",
            also_reads: &[],
        }),
    },
    AgentSpec {
        id: "hermes-agent",
        aliases: &["hermes"],
        label: "Hermes Agent",
        markers: &["HERMES_SESSION_ID"],
        session_var: Some("HERMES_SESSION_ID"),
        folders: Some(Folders {
            config: ".hermes",
            skills: ".hermes/skills",
            also_reads: &[],
        }),
    },
    AgentSpec {
        id: "kimi",
        aliases: &[],
        label: "Kimi Code",
        // named only by AI_AGENT or AGENT: it sets no variable of its own
        markers: &[],
        session_var: None,
        folders: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_name_names_one_agent() {
        let mut seen = HashSet::new();
        for agent in all() {
            for name in std::iter::once(&agent.id).chain(agent.aliases) {
                assert!(seen.insert(*name), "{name} names two agents");
            }
            for other in agent.folders.iter().flat_map(|f| f.also_reads) {
                assert!(find(other).is_some(), "{} reads {other}", agent.id);
            }
        }
        assert_eq!(all().len(), 15);
        assert_eq!(find("hermes").map(|a| a.id), Some("hermes-agent"));
        assert!(find("nobody").is_none());
    }
}
