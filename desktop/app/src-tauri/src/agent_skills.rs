//! The global `pinrail` skill, written into the agents found on this
//! computer: each keeps its own skills under its configuration folder, as
//! `<skills>/pinrail/SKILL.md`. The skill says how to submit through
//! Pinrail; the prompt or skill that sends an agent to it says when.
//!
//! A skill the app wrote says `managed-by: Pinrail` in its front matter, so
//! the app updates and removes only its own. A `pinrail` skill without that
//! line is the person's, and is left alone.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// The skill as the app writes it, with `{version}` for the app's version.
const SKILL: &str = include_str!("../skills/pinrail/SKILL.md");
const MARK: &str = "managed-by: Pinrail";

/// An agent the app knows: where its configuration folder is, and where it
/// keeps global skills, both under the home folder.
struct Agent {
    id: &'static str,
    name: &'static str,
    config: &'static str,
    skills: &'static str,
    /// another agent whose skills this one reads as well
    reads: Option<&'static str>,
}

const AGENTS: &[Agent] = &[
    Agent {
        id: "claude",
        name: "Claude Code",
        config: ".claude",
        skills: ".claude/skills",
        reads: None,
    },
    Agent {
        id: "codex",
        name: "Codex",
        config: ".codex",
        skills: ".codex/skills",
        reads: None,
    },
    Agent {
        id: "cursor",
        name: "Cursor",
        config: ".cursor",
        skills: ".cursor/skills",
        reads: None,
    },
    Agent {
        id: "gemini",
        name: "Gemini CLI",
        config: ".gemini",
        skills: ".gemini/skills",
        reads: None,
    },
    Agent {
        id: "opencode",
        name: "OpenCode",
        config: ".config/opencode",
        skills: ".config/opencode/skills",
        // OpenCode reads ~/.claude/skills too: a second copy would be a
        // second skill of the same name
        reads: Some("claude"),
    },
];

/// One agent, as Settings shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentStatus {
    pub id: &'static str,
    pub name: &'static str,
    /// its configuration folder is there
    pub found: bool,
    /// where the skill goes
    pub skill: String,
    pub state: State,
    /// for `covered`: the agent whose skill this one reads
    pub covered_by: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// no `pinrail` skill
    Absent,
    /// the app's skill, as this version writes it
    Connected,
    /// the app's skill, written by another version
    Outdated,
    /// a `pinrail` skill that is not the app's
    Theirs,
    /// none of its own, but it reads another agent's
    Covered,
}

/// The skill this version of the app writes.
pub fn skill_text(version: &str) -> String {
    SKILL.replace("{version}", version)
}

fn skill_path(home: &Path, agent: &Agent) -> PathBuf {
    home.join(agent.skills).join("pinrail").join("SKILL.md")
}

/// What is in an agent's place for the skill.
fn state_of(path: &Path, expected: &str) -> State {
    match fs::read_to_string(path) {
        Err(_) => State::Absent,
        Ok(text) if !managed(&text) => State::Theirs,
        Ok(text) if text == expected => State::Connected,
        Ok(_) => State::Outdated,
    }
}

/// Whether the front matter marks the skill as the app's.
fn managed(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("---\n") else {
        return false;
    };
    let front = rest.split("\n---").next().unwrap_or("");
    front.lines().any(|line| line.trim() == MARK)
}

/// Every agent the app knows, found or not.
pub fn status(home: &Path, version: &str) -> Vec<AgentStatus> {
    let expected = skill_text(version);
    let own: Vec<State> = AGENTS
        .iter()
        .map(|a| state_of(&skill_path(home, a), &expected))
        .collect();
    AGENTS
        .iter()
        .zip(&own)
        .map(|(agent, &state)| {
            let reader = agent
                .reads
                .and_then(|id| AGENTS.iter().position(|a| a.id == id))
                .filter(|&i| own[i] != State::Absent);
            let (state, covered_by) = match reader {
                Some(i) if state == State::Absent => (State::Covered, Some(AGENTS[i].name)),
                _ => (state, None),
            };
            AgentStatus {
                id: agent.id,
                name: agent.name,
                found: home.join(agent.config).is_dir(),
                skill: skill_path(home, agent).display().to_string(),
                state,
                covered_by,
            }
        })
        .collect()
}

fn find(id: &str) -> Result<&'static Agent, String> {
    AGENTS
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| format!("Pinrail does not know an agent called {id}"))
}

/// Writes the skill for an agent that is installed, or brings an older one
/// up to date. A `pinrail` skill of the person's own is never replaced.
pub fn connect(home: &Path, id: &str, version: &str) -> Result<(), String> {
    let agent = find(id)?;
    if !home.join(agent.config).is_dir() {
        return Err(format!(
            "{} is not installed: ~/{} is not there",
            agent.name, agent.config
        ));
    }
    let path = skill_path(home, agent);
    if state_of(&path, &skill_text(version)) == State::Theirs {
        return Err(format!(
            "{} is a skill of yours, not Pinrail's; it is left alone",
            path.display()
        ));
    }
    if let Some(covered) = status(home, version)
        .into_iter()
        .find(|a| a.id == id && a.state == State::Covered)
    {
        return Err(format!(
            "{} already reads {}'s skill; a second copy would be a second skill of the same name",
            agent.name,
            covered.covered_by.unwrap_or("another agent")
        ));
    }
    let dir = path.parent().expect("the skill's path has a folder");
    fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    // written beside it and renamed, so an agent never reads half a skill
    let partial = dir.join(".SKILL.md.partial");
    fs::write(&partial, skill_text(version))
        .map_err(|e| format!("writing {}: {e}", partial.display()))?;
    fs::rename(&partial, &path).map_err(|e| format!("writing {}: {e}", path.display()))
}

/// Removes the app's skill from an agent, and its folder when nothing else
/// is in it. A skill of the person's own stays.
pub fn disconnect(home: &Path, id: &str) -> Result<(), String> {
    let agent = find(id)?;
    let path = skill_path(home, agent);
    match fs::read_to_string(&path) {
        Err(_) => return Ok(()),
        Ok(text) if !managed(&text) => {
            return Err(format!(
                "{} is a skill of yours, not Pinrail's; it is left alone",
                path.display()
            ));
        }
        Ok(_) => {}
    }
    fs::remove_file(&path).map_err(|e| format!("removing {}: {e}", path.display()))?;
    if let Some(dir) = path.parent() {
        // only when empty: the person may keep files of their own beside it
        let _ = fs::remove_dir(dir);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home_with(agents: &[&str]) -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        for config in agents {
            fs::create_dir_all(home.path().join(config)).unwrap();
        }
        home
    }

    fn state(home: &Path, id: &str, version: &str) -> AgentStatus {
        status(home, version)
            .into_iter()
            .find(|a| a.id == id)
            .unwrap()
    }

    #[test]
    fn the_skill_in_the_repository_is_marked_as_the_apps() {
        let text = skill_text("1.2.3");
        assert!(managed(&text));
        assert!(text.contains("version: 1.2.3"));
        assert!(!text.contains("{version}"));
        assert!(text.starts_with("---\nname: pinrail\n"));
    }

    #[test]
    fn an_agent_is_found_by_its_configuration_folder() {
        let home = home_with(&[".claude", ".config/opencode"]);
        let found: Vec<_> = status(home.path(), "1.0.0")
            .into_iter()
            .filter(|a| a.found)
            .map(|a| a.id)
            .collect();
        assert_eq!(found, vec!["claude", "opencode"]);
        let claude = state(home.path(), "claude", "1.0.0");
        assert_eq!(claude.state, State::Absent);
        assert!(claude.skill.ends_with(".claude/skills/pinrail/SKILL.md"));
    }

    #[test]
    fn connecting_writes_the_skill_and_removing_takes_it_away() {
        let home = home_with(&[".codex"]);
        connect(home.path(), "codex", "1.0.0").unwrap();
        let path = home.path().join(".codex/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(&path).unwrap(), skill_text("1.0.0"));
        assert_eq!(state(home.path(), "codex", "1.0.0").state, State::Connected);
        assert!(
            !home
                .path()
                .join(".codex/skills/pinrail/.SKILL.md.partial")
                .exists()
        );

        disconnect(home.path(), "codex").unwrap();
        assert!(!path.exists());
        assert!(!home.path().join(".codex/skills/pinrail").exists());
        assert!(
            home.path().join(".codex/skills").is_dir(),
            "the agent's skills folder stays"
        );
        assert_eq!(state(home.path(), "codex", "1.0.0").state, State::Absent);
    }

    #[test]
    fn an_agent_that_is_not_installed_is_not_connected() {
        let home = home_with(&[]);
        let refused = connect(home.path(), "gemini", "1.0.0").unwrap_err();
        assert!(refused.contains("Gemini CLI is not installed"), "{refused}");
        assert!(!home.path().join(".gemini").exists());
    }

    #[test]
    fn a_skill_from_another_version_is_outdated_and_connecting_updates_it() {
        let home = home_with(&[".claude"]);
        connect(home.path(), "claude", "1.0.0").unwrap();
        assert_eq!(state(home.path(), "claude", "1.1.0").state, State::Outdated);
        connect(home.path(), "claude", "1.1.0").unwrap();
        assert_eq!(
            state(home.path(), "claude", "1.1.0").state,
            State::Connected
        );
    }

    #[test]
    fn a_pinrail_skill_of_the_persons_own_is_left_alone() {
        let home = home_with(&[".cursor"]);
        let dir = home.path().join(".cursor/skills/pinrail");
        fs::create_dir_all(&dir).unwrap();
        let theirs = "---\nname: pinrail\ndescription: mine\n---\n\nMy own rules.\n";
        fs::write(dir.join("SKILL.md"), theirs).unwrap();
        fs::write(dir.join("notes.md"), "beside it").unwrap();

        assert_eq!(state(home.path(), "cursor", "1.0.0").state, State::Theirs);
        assert!(
            connect(home.path(), "cursor", "1.0.0")
                .unwrap_err()
                .contains("a skill of yours")
        );
        assert!(
            disconnect(home.path(), "cursor")
                .unwrap_err()
                .contains("a skill of yours")
        );
        assert_eq!(fs::read_to_string(dir.join("SKILL.md")).unwrap(), theirs);
    }

    #[test]
    fn the_mark_counts_only_in_the_front_matter() {
        assert!(!managed("# Notes\n\nmanaged-by: Pinrail\n"));
        assert!(!managed("---\nname: pinrail\n---\nmanaged-by: Pinrail\n"));
        assert!(managed(
            "---\nname: pinrail\nmetadata:\n  managed-by: Pinrail\n---\nbody\n"
        ));
    }

    #[test]
    fn removing_keeps_files_of_the_persons_own_beside_the_skill() {
        let home = home_with(&[".claude"]);
        connect(home.path(), "claude", "1.0.0").unwrap();
        let dir = home.path().join(".claude/skills/pinrail");
        fs::write(dir.join("notes.md"), "mine").unwrap();
        disconnect(home.path(), "claude").unwrap();
        assert!(!dir.join("SKILL.md").exists());
        assert!(dir.join("notes.md").exists());
    }

    #[test]
    fn opencode_is_covered_by_claude_codes_skill_and_needs_none_of_its_own() {
        let home = home_with(&[".claude", ".config/opencode"]);
        assert_eq!(state(home.path(), "opencode", "1.0.0").state, State::Absent);
        connect(home.path(), "claude", "1.0.0").unwrap();
        let opencode = state(home.path(), "opencode", "1.0.0");
        assert_eq!(opencode.state, State::Covered);
        assert_eq!(opencode.covered_by, Some("Claude Code"));
        let refused = connect(home.path(), "opencode", "1.0.0").unwrap_err();
        assert!(
            refused.contains("already reads Claude Code's skill"),
            "{refused}"
        );
        assert!(!home.path().join(".config/opencode/skills/pinrail").exists());
    }

    #[test]
    fn opencode_gets_its_own_skill_when_claude_code_has_none() {
        let home = home_with(&[".config/opencode"]);
        connect(home.path(), "opencode", "1.0.0").unwrap();
        assert_eq!(
            state(home.path(), "opencode", "1.0.0").state,
            State::Connected
        );
    }
}
