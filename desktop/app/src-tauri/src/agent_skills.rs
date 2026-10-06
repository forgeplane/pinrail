//! The global `pinrail` skill, written into the agents found on this
//! computer: each keeps its own skills under its configuration folder, as
//! `<skills>/pinrail/`, with SKILL.md and its references beside it. The
//! skill says how to submit through Pinrail and how to build a plugin; the
//! prompt or skill that sends an agent to it says when. Its files are the
//! `skill/` folder of the repository, embedded at build.
//!
//! A skill the app wrote says `managed-by: Pinrail` in its front matter, so
//! the app updates and removes only its own. A `pinrail` skill without that
//! line is the person's, and is left alone. The front matter also carries
//! a SHA-256 of the skill's files: a copy with another one is outdated.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// The skill's files as the app writes them: each one's path in the
/// skill's folder, such as `references/asking.md`, and its text.
const FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/skill_files.rs"));
/// The SHA-256 of the files, which SKILL.md's front matter carries.
const SHA: &str = include!(concat!(env!("OUT_DIR"), "/skill_sha.rs"));
const MARK: &str = "managed-by: Pinrail";
/// The folder of the skill's references, which is all the app's.
const REFERENCES: &str = "references";

/// An agent the app knows: where its configuration folder is, and where it
/// keeps global skills, both under the home folder.
struct Agent {
    id: &'static str,
    name: &'static str,
    config: &'static str,
    skills: &'static str,
    /// other agents whose skills this one reads as well
    reads: &'static [&'static str],
}

const AGENTS: &[Agent] = &[
    Agent {
        id: "claude",
        name: "Claude Code",
        config: ".claude",
        skills: ".claude/skills",
        reads: &[],
    },
    Agent {
        id: "codex",
        name: "Codex",
        config: ".codex",
        skills: ".codex/skills",
        reads: &[],
    },
    Agent {
        id: "cursor",
        name: "Cursor",
        config: ".cursor",
        skills: ".cursor/skills",
        reads: &[],
    },
    Agent {
        id: "antigravity",
        name: "Antigravity CLI",
        config: ".gemini",
        skills: ".gemini/config/skills",
        reads: &[],
    },
    Agent {
        id: "opencode",
        name: "OpenCode",
        config: ".config/opencode",
        skills: ".config/opencode/skills",
        // OpenCode reads ~/.claude/skills too: a second copy would be a
        // second skill of the same name
        reads: &["claude"],
    },
    Agent {
        id: "grok",
        name: "Grok CLI",
        config: ".grok",
        skills: ".grok/skills",
        // Grok reads ~/.claude/skills and ~/.cursor/skills too, unless the
        // person turns that off
        reads: &["claude", "cursor"],
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
    /// the app's skill, with other contents than this version's
    Outdated,
    /// a `pinrail` skill that is not the app's
    Theirs,
    /// none of its own, but it reads another agent's
    Covered,
}

/// The skill's SKILL.md as this version of the app writes it.
pub fn skill_text() -> &'static str {
    FILES
        .iter()
        .find(|(path, _)| *path == "SKILL.md")
        .map(|(_, text)| *text)
        .expect("the skill has a SKILL.md")
}

/// SKILL.md on its own, to give an agent the app does not know: each link
/// to a reference, which is not beside it there, becomes the `pinrail docs`
/// command that prints the same text.
pub fn skill_to_copy() -> String {
    let mut out = String::new();
    let mut rest = skill_text();
    while let Some(at) = rest.find("](references/") {
        let Some(open) = rest[..at].rfind('[') else {
            break;
        };
        let target = &rest[at + "](references/".len()..];
        let Some(end) = target.find(".md)") else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&format!("`pinrail docs {}`", &target[..end]));
        rest = &target[end + ".md)".len()..];
    }
    out.push_str(rest);
    out
}

fn skill_path(home: &Path, agent: &Agent) -> PathBuf {
    home.join(agent.skills).join("pinrail").join("SKILL.md")
}

/// What is in an agent's place for the skill.
fn state_of(path: &Path) -> State {
    match fs::read_to_string(path) {
        Err(_) => State::Absent,
        Ok(text) if !managed(&text) => State::Theirs,
        Ok(text) if sha(&text) == Some(SHA) => State::Connected,
        Ok(_) => State::Outdated,
    }
}

/// The lines of the front matter, if the text has one.
fn front_matter(text: &str) -> Option<impl Iterator<Item = &str>> {
    let rest = text.strip_prefix("---\n")?;
    Some(
        rest.split("\n---")
            .next()
            .unwrap_or("")
            .lines()
            .map(str::trim),
    )
}

/// Whether the front matter marks the skill as the app's.
fn managed(text: &str) -> bool {
    front_matter(text).is_some_and(|mut lines| lines.any(|line| line == MARK))
}

/// The SHA-256 the front matter gives for the skill's files.
fn sha(text: &str) -> Option<&str> {
    front_matter(text)?.find_map(|line| line.strip_prefix("sha:").map(str::trim))
}

/// Every agent the app knows, found or not.
pub fn status(home: &Path) -> Vec<AgentStatus> {
    let own: Vec<State> = AGENTS
        .iter()
        .map(|a| state_of(&skill_path(home, a)))
        .collect();
    AGENTS
        .iter()
        .zip(&own)
        .map(|(agent, &state)| {
            let reader = agent
                .reads
                .iter()
                .filter_map(|id| AGENTS.iter().position(|a| a.id == *id))
                .find(|&i| own[i] != State::Absent);
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
pub fn connect(home: &Path, id: &str) -> Result<(), String> {
    let agent = find(id)?;
    if !home.join(agent.config).is_dir() {
        return Err(format!(
            "{} is not installed: ~/{} is not there",
            agent.name, agent.config
        ));
    }
    let path = skill_path(home, agent);
    if state_of(&path) == State::Theirs {
        return Err(format!(
            "{} is a skill of yours, not Pinrail's; it is left alone",
            path.display()
        ));
    }
    if let Some(covered) = status(home)
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
    // the references of an older version go, and this version's take their place
    remove_references(dir)?;
    // SKILL.md last, so the skill counts as connected only once it is whole
    let mut files: Vec<&(&str, &str)> = FILES.iter().filter(|(p, _)| *p != "SKILL.md").collect();
    files.extend(FILES.iter().filter(|(p, _)| *p == "SKILL.md"));
    for (rel, text) in files {
        write(&dir.join(rel), text)?;
    }
    Ok(())
}

/// Writes one file beside where it goes and renames it, so an agent never
/// reads half of it.
fn write(path: &Path, text: &str) -> Result<(), String> {
    let folder = path.parent().expect("a skill file has a folder");
    fs::create_dir_all(folder).map_err(|e| format!("creating {}: {e}", folder.display()))?;
    let name = path.file_name().unwrap().to_string_lossy();
    let partial = folder.join(format!(".{name}.partial"));
    fs::write(&partial, text).map_err(|e| format!("writing {}: {e}", partial.display()))?;
    fs::rename(&partial, path).map_err(|e| format!("writing {}: {e}", path.display()))
}

fn remove_references(dir: &Path) -> Result<(), String> {
    let references = dir.join(REFERENCES);
    match fs::remove_dir_all(&references) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("removing {}: {e}", references.display()))
        }
        _ => Ok(()),
    }
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
        remove_references(dir)?;
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

    fn state(home: &Path, id: &str) -> AgentStatus {
        status(home).into_iter().find(|a| a.id == id).unwrap()
    }

    #[test]
    fn the_skill_in_the_repository_is_marked_as_the_apps() {
        let text = skill_text();
        assert!(managed(text));
        assert_eq!(sha(text), Some(SHA));
        assert_eq!(SHA.len(), 64);
        assert!(text.starts_with("---\nname: pinrail\n"));
    }

    #[test]
    fn the_skill_to_copy_names_the_commands_for_its_references() {
        let text = skill_to_copy();
        assert!(
            text.contains("- `pinrail docs asking`: Submit a review"),
            "{text}"
        );
        assert!(!text.contains("references/"), "{text}");
    }

    #[test]
    fn the_skill_carries_its_references_with_the_manifest_schema_in_place() {
        let paths: Vec<_> = FILES.iter().map(|(p, _)| *p).collect();
        assert!(paths.contains(&"references/asking.md"), "{paths:?}");
        assert!(
            paths.contains(&"references/plugins/building.md"),
            "{paths:?}"
        );
        let manifest = FILES
            .iter()
            .find(|(p, _)| *p == "references/plugins/building/manifest.md")
            .unwrap()
            .1;
        assert!(!manifest.contains("{{manifest_schema}}"));
        // the build fills in every placeholder, the SDK's version among them
        for (path, text) in FILES {
            assert!(!text.contains("{{"), "{path}: a placeholder left");
        }
        let dev_shell = FILES
            .iter()
            .find(|(p, _)| *p == "references/plugins/building/dev-shell.md")
            .unwrap()
            .1;
        assert!(dev_shell.contains("npx pinrail-sdk@1."), "{dev_shell}");
        assert!(manifest.contains("\"$schema\""), "{manifest}");
        // every link between the files leads to one of them
        for (path, text) in FILES {
            let folder = Path::new(path).parent().unwrap();
            for target in text
                .split("](")
                .skip(1)
                .filter_map(|t| t.split_once(')'))
                .map(|(t, _)| t)
            {
                if target.ends_with(".md") && !target.contains("://") {
                    let joined = folder.join(target);
                    let mut resolved = PathBuf::new();
                    for part in joined.components() {
                        match part {
                            std::path::Component::ParentDir => {
                                resolved.pop();
                            }
                            other => resolved.push(other),
                        }
                    }
                    let resolved = resolved.to_string_lossy().replace('\\', "/");
                    assert!(
                        paths.contains(&resolved.as_str()),
                        "{path}: {target} is not in the skill"
                    );
                }
            }
        }
    }

    #[test]
    fn an_agent_is_found_by_its_configuration_folder() {
        let home = home_with(&[".claude", ".config/opencode"]);
        let found: Vec<_> = status(home.path())
            .into_iter()
            .filter(|a| a.found)
            .map(|a| a.id)
            .collect();
        assert_eq!(found, vec!["claude", "opencode"]);
        let claude = state(home.path(), "claude");
        assert_eq!(claude.state, State::Absent);
        assert!(claude.skill.ends_with(".claude/skills/pinrail/SKILL.md"));
    }

    #[test]
    fn connecting_writes_the_skill_and_removing_takes_it_away() {
        let home = home_with(&[".codex"]);
        connect(home.path(), "codex").unwrap();
        let path = home.path().join(".codex/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(&path).unwrap(), skill_text());
        let building = home
            .path()
            .join(".codex/skills/pinrail/references/plugins/building.md");
        assert!(building.is_file());
        assert_eq!(state(home.path(), "codex").state, State::Connected);
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
        assert_eq!(state(home.path(), "codex").state, State::Absent);
    }

    #[test]
    fn an_agent_that_is_not_installed_is_not_connected() {
        let home = home_with(&[]);
        let refused = connect(home.path(), "antigravity").unwrap_err();
        assert!(
            refused.contains("Antigravity CLI is not installed"),
            "{refused}"
        );
        assert!(!home.path().join(".gemini").exists());
    }

    #[test]
    fn a_skill_with_other_contents_is_outdated_and_connecting_updates_it() {
        let home = home_with(&[".claude"]);
        connect(home.path(), "claude").unwrap();
        let dir = home.path().join(".claude/skills/pinrail");
        // as an older app wrote it: another hash, and a reference since dropped
        let older = skill_text().replace(SHA, "0123");
        fs::write(dir.join("SKILL.md"), &older).unwrap();
        fs::write(dir.join("references/dropped.md"), "# Dropped\n").unwrap();
        assert_eq!(state(home.path(), "claude").state, State::Outdated);

        connect(home.path(), "claude").unwrap();
        assert_eq!(state(home.path(), "claude").state, State::Connected);
        assert_eq!(
            fs::read_to_string(dir.join("SKILL.md")).unwrap(),
            skill_text()
        );
        assert!(!dir.join("references/dropped.md").exists());
    }

    #[test]
    fn only_the_hash_decides_whether_a_skill_is_current() {
        let home = home_with(&[".claude"]);
        connect(home.path(), "claude").unwrap();
        // a reference changed by hand does not make the skill outdated, nor
        // does any other change that keeps the hash
        let dir = home.path().join(".claude/skills/pinrail");
        fs::write(dir.join("references/asking.md"), "# Asking\n").unwrap();
        assert_eq!(state(home.path(), "claude").state, State::Connected);
    }

    #[test]
    fn a_pinrail_skill_of_the_persons_own_is_left_alone() {
        let home = home_with(&[".cursor"]);
        let dir = home.path().join(".cursor/skills/pinrail");
        fs::create_dir_all(&dir).unwrap();
        let theirs = "---\nname: pinrail\ndescription: mine\n---\n\nMy own rules.\n";
        fs::write(dir.join("SKILL.md"), theirs).unwrap();
        fs::write(dir.join("notes.md"), "beside it").unwrap();

        assert_eq!(state(home.path(), "cursor").state, State::Theirs);
        assert!(
            connect(home.path(), "cursor")
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
        connect(home.path(), "claude").unwrap();
        let dir = home.path().join(".claude/skills/pinrail");
        fs::write(dir.join("notes.md"), "mine").unwrap();
        disconnect(home.path(), "claude").unwrap();
        assert!(!dir.join("SKILL.md").exists());
        assert!(!dir.join("references").exists());
        assert!(dir.join("notes.md").exists());
    }

    #[test]
    fn opencode_is_covered_by_claude_codes_skill_and_needs_none_of_its_own() {
        let home = home_with(&[".claude", ".config/opencode"]);
        assert_eq!(state(home.path(), "opencode").state, State::Absent);
        connect(home.path(), "claude").unwrap();
        let opencode = state(home.path(), "opencode");
        assert_eq!(opencode.state, State::Covered);
        assert_eq!(opencode.covered_by, Some("Claude Code"));
        let refused = connect(home.path(), "opencode").unwrap_err();
        assert!(
            refused.contains("already reads Claude Code's skill"),
            "{refused}"
        );
        assert!(!home.path().join(".config/opencode/skills/pinrail").exists());
    }

    #[test]
    fn antigravity_keeps_its_skills_in_geminis_config_folder() {
        let home = home_with(&[".gemini"]);
        connect(home.path(), "antigravity").unwrap();
        let path = home.path().join(".gemini/config/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(path).unwrap(), skill_text());
        assert_eq!(state(home.path(), "antigravity").state, State::Connected);
    }

    #[test]
    fn grok_is_found_by_its_folder_and_covered_by_claude_code_or_cursor() {
        let home = home_with(&[".grok", ".cursor"]);
        let grok = state(home.path(), "grok");
        assert!(grok.found);
        assert_eq!(grok.state, State::Absent);
        assert!(grok.skill.ends_with(".grok/skills/pinrail/SKILL.md"));

        connect(home.path(), "cursor").unwrap();
        let grok = state(home.path(), "grok");
        assert_eq!(grok.state, State::Covered);
        assert_eq!(grok.covered_by, Some("Cursor"));
        assert!(connect(home.path(), "grok").is_err());

        disconnect(home.path(), "cursor").unwrap();
        connect(home.path(), "grok").unwrap();
        assert_eq!(state(home.path(), "grok").state, State::Connected);
    }

    #[test]
    fn opencode_gets_its_own_skill_when_claude_code_has_none() {
        let home = home_with(&[".config/opencode"]);
        connect(home.path(), "opencode").unwrap();
        assert_eq!(state(home.path(), "opencode").state, State::Connected);
    }
}
