//! Where each agent keeps its configuration and its global skills, and
//! the skill's text for the person's settings.

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

/// The skill's description, in place of `{description}` in its front
/// matter, with sessions and questions on and off.
const DESCRIPTION: &str = "Ask the person questions or submit a review to them through Pinrail and wait for their answer, or build a Pinrail plugin. Use when you need an answer only the person can give, or when a prompt, a skill or your instructions tell you to ask, check with, or get approval from the person through Pinrail.";
const DESCRIPTION_WITHOUT_SESSIONS: &str = "Submit a review to the person through Pinrail and wait for their decision, or build a Pinrail plugin. Use when you need a decision only the person can make, or when a prompt, a skill or your instructions tell you to check with, or get approval from, the person through Pinrail.";

/// The skill's text for one setting of Settings › Labs: a part between
/// `<!-- sessions -->` and `<!-- end sessions -->` stays only while
/// sessions and questions are on, and a part between `<!-- no sessions -->`
/// and `<!-- end no sessions -->` only while they are off. The marker
/// lines themselves go, and `{description}` becomes the description that
/// fits.
pub fn skill_text(text: &str, sessions: bool) -> String {
    let mut out = String::with_capacity(text.len());
    // the part being read: None outside a marked part, or whether it stays
    let mut keep: Option<bool> = None;
    for line in text.split_inclusive('\n') {
        match line.trim_end() {
            "<!-- sessions -->" => keep = Some(sessions),
            "<!-- no sessions -->" => keep = Some(!sessions),
            "<!-- end sessions -->" | "<!-- end no sessions -->" => keep = None,
            _ if keep != Some(false) => out.push_str(line),
            _ => {}
        }
    }
    let description = if sessions {
        DESCRIPTION
    } else {
        DESCRIPTION_WITHOUT_SESSIONS
    };
    out.replace("{description}", description)
}

#[cfg(test)]
mod tests {
    use super::skill_text;

    const TEXT: &str = "intro\n<!-- sessions -->\nask\n<!-- end sessions -->\n<!-- no sessions -->\nsubmit only\n<!-- end no sessions -->\nrules\n";

    #[test]
    fn the_skill_keeps_the_parts_for_sessions_on_or_off() {
        assert_eq!(skill_text(TEXT, true), "intro\nask\nrules\n");
        assert_eq!(skill_text(TEXT, false), "intro\nsubmit only\nrules\n");
        // a text without markers is left as it is
        assert_eq!(skill_text("a\nb\n", false), "a\nb\n");
    }

    #[test]
    fn the_description_fits_the_setting() {
        let on = skill_text("description: {description}\n", true);
        let off = skill_text("description: {description}\n", false);
        assert!(
            on.starts_with("description: Ask the person questions"),
            "{on}"
        );
        assert!(off.starts_with("description: Submit a review"), "{off}");
        assert!(!off.contains("questions or"), "{off}");
    }
}
