//! Where each agent keeps its configuration and its global skills.

/// An agent the app knows: where its configuration folder is, and where it
/// keeps global skills, both under the home folder.
pub struct Agent {
    pub id: &'static str,
    pub name: &'static str,
    pub config: &'static str,
    pub skills: &'static str,
    /// other agents whose skill folders this one also reads; the skill is
    /// still installed in this agent's own folder
    pub also_reads: &'static [&'static str],
}

pub const AGENTS: &[Agent] = &[
    Agent {
        id: "claude",
        name: "Claude Code",
        config: ".claude",
        skills: ".claude/skills",
        also_reads: &[],
    },
    Agent {
        id: "codex",
        name: "Codex",
        config: ".codex",
        skills: ".codex/skills",
        also_reads: &[],
    },
    Agent {
        id: "cursor",
        name: "Cursor",
        config: ".cursor",
        skills: ".cursor/skills",
        also_reads: &[],
    },
    Agent {
        id: "antigravity",
        name: "Antigravity CLI",
        config: ".gemini",
        skills: ".gemini/config/skills",
        also_reads: &[],
    },
    Agent {
        id: "opencode",
        name: "OpenCode",
        config: ".config/opencode",
        skills: ".config/opencode/skills",
        // OpenCode also reads ~/.claude/skills. It loads one skill per
        // name, so installing in both folders does not duplicate it.
        also_reads: &["claude"],
    },
    Agent {
        id: "grok",
        name: "Grok CLI",
        config: ".grok",
        skills: ".grok/skills",
        // Grok also reads ~/.claude/skills and ~/.cursor/skills, unless the
        // person turns that off, and loads one skill per name
        also_reads: &["claude", "cursor"],
    },
    Agent {
        id: "pi",
        name: "Pi",
        config: ".pi/agent",
        skills: ".pi/agent/skills",
        also_reads: &[],
    },
    Agent {
        id: "goose",
        name: "Goose",
        config: ".config/goose",
        // not ~/.agents/skills, which other agents read too
        skills: ".config/goose/skills",
        // Goose also reads ~/.claude/skills and loads the first skill of a
        // name it finds, so installing in both folders does not duplicate it
        also_reads: &["claude"],
    },
    Agent {
        id: "amp",
        name: "Amp",
        config: ".config/amp",
        // not ~/.config/agents/skills, which other agents read too
        skills: ".config/amp/skills",
        // Amp also reads ~/.claude/skills and loads the first skill of a
        // name it finds, so installing in both folders does not duplicate it
        also_reads: &["claude"],
    },
    Agent {
        id: "copilot",
        name: "Copilot CLI",
        config: ".copilot",
        skills: ".copilot/skills",
        also_reads: &[],
    },
    Agent {
        id: "kiro",
        name: "Kiro CLI",
        config: ".kiro",
        skills: ".kiro/skills",
        also_reads: &[],
    },
    Agent {
        id: "augment",
        name: "Augment CLI",
        config: ".augment",
        skills: ".augment/skills",
        // Augment also reads ~/.claude/skills and loads the first skill of
        // a name it finds, so installing in both folders does not duplicate it
        also_reads: &["claude"],
    },
    Agent {
        id: "cline",
        name: "Cline",
        config: ".cline",
        skills: ".cline/skills",
        also_reads: &[],
    },
    Agent {
        id: "hermes",
        name: "Hermes Agent",
        config: ".hermes",
        skills: ".hermes/skills",
        also_reads: &[],
    },
];
