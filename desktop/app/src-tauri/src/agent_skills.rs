//! The global `pinrail` skill, written into the agents found on this
//! computer: each keeps its own skills under its configuration folder, as
//! `<skills>/pinrail/`, with SKILL.md and its references beside it. The
//! skill says how to submit through Pinrail and how to build a plugin; the
//! prompt or skill that sends an agent to it says when. Its files are the
//! `skill/` folder of the repository, embedded at build.
//!
//! To those files the app adds what is installed: SKILL.md ends with a list
//! of the installed plugins, each with when to use it, and the skill of each
//! plugin that ships one goes under `plugins/<name>/`, linked from the list.
//!
//! A skill the app wrote says `managed-by: Pinrail` in its front matter, so
//! the app updates and removes only its own. A `pinrail` skill without that
//! line is the person's, and is left alone. The front matter also carries
//! a SHA-256 of all the files written, so a copy with another one is
//! outdated, and as `base` the SHA-256 of the app's own files, so a copy
//! that differs only in its plugins can be told from one an older app wrote.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

/// The app's own files of the skill: each one's path in the skill's folder,
/// such as `references/building.md`, and its text, SKILL.md with `{sha}`
/// and `{base}` still in its front matter.
const FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/skill_files.rs"));
/// The SHA-256 of the app's own files.
const BASE: &str = include!(concat!(env!("OUT_DIR"), "/skill_sha.rs"));
const MARK: &str = "managed-by: Pinrail";
/// The folders of the skill's references and of its plugins' skills, which
/// are all the app's.
const REFERENCES: &str = "references";
const PLUGINS: &str = "plugins";

/// An installed plugin as the skill names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPlugin {
    pub name: String,
    pub title: String,
    /// when an agent should ask with it, or what it is
    pub use_when: Option<String>,
    /// the folder of its skill and the files in it, when its skill goes to
    /// the agents
    pub skill: Option<(PathBuf, Vec<String>)>,
}

/// The skill as this app writes it with the plugins installed now: its
/// files, and the SHA-256 that names them.
#[derive(Debug, Clone)]
pub struct Skill {
    files: Vec<(String, Vec<u8>)>,
    sha: String,
}

impl Skill {
    /// The app's files, with the skills the `plugins` ship under `plugins/`
    /// and SKILL.md naming those plugins in place of `{plugin_skills}`. The
    /// other plugins are for `pinrail plugins` to list. A plugin skill whose
    /// files cannot be read is left out, with its plugin.
    pub fn with(plugins: &[InstalledPlugin]) -> Skill {
        let mut extra: Vec<(String, Vec<u8>)> = Vec::new();
        let mut list = String::new();
        for plugin in plugins {
            let read: Option<Vec<(String, Vec<u8>)>> =
                plugin.skill.as_ref().and_then(|(dir, files)| {
                    files
                        .iter()
                        .map(|rel| {
                            let path = dir.join(rel);
                            // the files of a skill are below its folder
                            (!rel.split('/').any(|part| part == ".." || part.is_empty()))
                                .then(|| fs::read(&path).ok())
                                .flatten()
                                .map(|bytes| (format!("{PLUGINS}/{}/{rel}", plugin.name), bytes))
                        })
                        .collect()
                });
            if let Some(files) = read {
                list.push_str(&format!(
                    "- `{}`: [its skill]({PLUGINS}/{}/SKILL.md)\n",
                    plugin.name, plugin.name
                ));
                extra.extend(files);
            }
        }
        let named = if list.is_empty() {
            String::new()
        } else {
            format!(
                "\n\nThe installed plugins with a skill:\n\n{}",
                list.trim_end()
            )
        };
        let mut files: Vec<(String, Vec<u8>)> = FILES
            .iter()
            .map(|(rel, text)| {
                let text = if *rel == "SKILL.md" {
                    text.replace("{plugin_skills}", &named)
                } else {
                    text.to_string()
                };
                (rel.to_string(), text.into_bytes())
            })
            .collect();
        files.extend(extra);
        files.sort_by(|a, b| a.0.cmp(&b.0));
        // all the files as they are written, SKILL.md with its placeholders
        let mut hash = Sha256::new();
        for (rel, bytes) in &files {
            hash.update(rel.as_bytes());
            hash.update([0]);
            hash.update(bytes);
            hash.update([0]);
        }
        let sha: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        for (rel, bytes) in &mut files {
            if rel == "SKILL.md" {
                let text = String::from_utf8_lossy(bytes)
                    .replace("{sha}", &sha)
                    .replace("{base}", BASE);
                *bytes = text.into_bytes();
            }
        }
        Skill { files, sha }
    }

    /// SKILL.md on its own, to give an agent the app does not know: each
    /// link to a reference or to a plugin's skill, which is not beside it
    /// there, becomes the command that prints the same text.
    pub fn to_copy(&self) -> String {
        let text = links_to_commands(&self.text(), "references/", ".md", |name| {
            format!("pinrail docs {name}")
        });
        links_to_commands(&text, &format!("{PLUGINS}/"), "/SKILL.md", |name| {
            format!("pinrail plugins skill {name}")
        })
    }

    /// SKILL.md as it is written.
    pub fn text(&self) -> String {
        self.files
            .iter()
            .find(|(rel, _)| rel == "SKILL.md")
            .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
            .expect("the skill has a SKILL.md")
    }
}

/// An agent the app knows: where its configuration folder is, and where it
/// keeps global skills, both under the home folder.
struct Agent {
    id: &'static str,
    name: &'static str,
    config: &'static str,
    skills: &'static str,
    /// other agents whose skill folders this one also reads; the skill is
    /// still installed in this agent's own folder
    also_reads: &'static [&'static str],
}

const AGENTS: &[Agent] = &[
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
    /// the other agents whose skill folders this one also reads
    pub also_reads: Vec<&'static str>,
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
}

/// Replaces each markdown link whose target starts with `prefix` and ends
/// with `suffix` by the command `command` makes of the part between.
fn links_to_commands(
    text: &str,
    prefix: &str,
    suffix: &str,
    command: impl Fn(&str) -> String,
) -> String {
    let mut out = String::new();
    let mut rest = text;
    let opening = format!("]({prefix}");
    while let Some(at) = rest.find(&opening) {
        let Some(open) = rest[..at].rfind('[') else {
            break;
        };
        let target = &rest[at + opening.len()..];
        let Some(end) = target.find(&format!("{suffix})")) else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&format!("`{}`", command(&target[..end])));
        rest = &target[end + suffix.len() + 1..];
    }
    out.push_str(rest);
    out
}

fn skill_path(home: &Path, agent: &Agent) -> PathBuf {
    home.join(agent.skills).join("pinrail").join("SKILL.md")
}

/// What is in an agent's place for the skill.
fn state_of(path: &Path, skill: &Skill) -> State {
    match fs::read_to_string(path) {
        Err(_) => State::Absent,
        Ok(text) if !managed(&text) => State::Theirs,
        Ok(text) if sha(&text, "sha") == Some(skill.sha.as_str()) => State::Connected,
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

/// A SHA-256 the front matter gives: `sha` for all the skill's files,
/// `base` for the app's own.
fn sha<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let key = format!("{key}:");
    front_matter(text)?.find_map(|line| line.strip_prefix(key.as_str()).map(str::trim))
}

/// Every agent the app knows, found or not.
pub fn status(home: &Path, skill: &Skill) -> Vec<AgentStatus> {
    AGENTS
        .iter()
        .map(|agent| AgentStatus {
            id: agent.id,
            name: agent.name,
            found: home.join(agent.config).is_dir(),
            skill: skill_path(home, agent).display().to_string(),
            state: state_of(&skill_path(home, agent), skill),
            also_reads: agent
                .also_reads
                .iter()
                .filter_map(|id| AGENTS.iter().find(|a| a.id == *id))
                .map(|a| a.name)
                .collect(),
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
pub fn connect(home: &Path, id: &str, skill: &Skill) -> Result<(), String> {
    let agent = find(id)?;
    if !home.join(agent.config).is_dir() {
        return Err(format!(
            "{} is not installed: ~/{} is not there",
            agent.name, agent.config
        ));
    }
    let path = skill_path(home, agent);
    if state_of(&path, skill) == State::Theirs {
        return Err(format!(
            "{} is a skill of yours, not Pinrail's; it is left alone",
            path.display()
        ));
    }
    let dir = path.parent().expect("the skill's path has a folder");
    // the references and plugin skills written before go, and these take
    // their place: a plugin removed since is left out
    remove_generated(dir)?;
    // SKILL.md last, so the skill counts as connected only once it is whole
    let mut files: Vec<&(String, Vec<u8>)> = skill
        .files
        .iter()
        .filter(|(p, _)| p != "SKILL.md")
        .collect();
    files.extend(skill.files.iter().filter(|(p, _)| p == "SKILL.md"));
    for (rel, bytes) in files {
        write(&dir.join(rel), bytes)?;
    }
    Ok(())
}

/// Brings every agent's outdated skill up to date, and returns the names of
/// the agents it updated. An agent whose skill is missing, current or the
/// person's own is left as it is.
pub fn update_outdated(home: &Path, skill: &Skill) -> Vec<&'static str> {
    status(home, skill)
        .into_iter()
        .filter(|a| a.found && a.state == State::Outdated)
        .filter(|a| connect(home, a.id, skill).is_ok())
        .map(|a| a.name)
        .collect()
}

/// Rewrites each agent's skill that differs from `skill` only in its
/// plugins, after a plugin was installed, updated or removed, and returns
/// the names of the agents it rewrote. A skill an older app wrote waits for
/// the person, or for the update at start when they asked for one.
pub fn refresh_plugins(home: &Path, skill: &Skill) -> Vec<&'static str> {
    AGENTS
        .iter()
        .filter(|agent| home.join(agent.config).is_dir())
        .filter(|agent| {
            let path = skill_path(home, agent);
            state_of(&path, skill) == State::Outdated
                && fs::read_to_string(&path).is_ok_and(|text| sha(&text, "base") == Some(BASE))
        })
        .filter(|agent| connect(home, agent.id, skill).is_ok())
        .map(|agent| agent.name)
        .collect()
}

/// Writes one file beside where it goes and renames it, so an agent never
/// reads half of it.
fn write(path: &Path, text: &[u8]) -> Result<(), String> {
    let folder = path.parent().expect("a skill file has a folder");
    fs::create_dir_all(folder).map_err(|e| format!("creating {}: {e}", folder.display()))?;
    let name = path.file_name().unwrap().to_string_lossy();
    let partial = folder.join(format!(".{name}.partial"));
    fs::write(&partial, text).map_err(|e| format!("writing {}: {e}", partial.display()))?;
    fs::rename(&partial, path).map_err(|e| format!("writing {}: {e}", path.display()))
}

/// Removes the folders the app writes beside SKILL.md: its references and
/// the plugins' skills.
fn remove_generated(dir: &Path) -> Result<(), String> {
    for folder in [REFERENCES, PLUGINS] {
        let path = dir.join(folder);
        match fs::remove_dir_all(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("removing {}: {e}", path.display()));
            }
            _ => {}
        }
    }
    Ok(())
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
        remove_generated(dir)?;
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

    /// The skill with no plugins installed.
    fn none() -> Skill {
        Skill::with(&[])
    }

    fn state(home: &Path, id: &str) -> AgentStatus {
        status(home, &none())
            .into_iter()
            .find(|a| a.id == id)
            .unwrap()
    }

    #[test]
    fn the_skill_in_the_repository_is_marked_as_the_apps() {
        let skill = none();
        let text = skill.text();
        assert!(managed(&text));
        assert_eq!(sha(&text, "sha"), Some(skill.sha.as_str()));
        assert_eq!(sha(&text, "base"), Some(BASE));
        assert_eq!(BASE.len(), 64);
        assert!(text.starts_with("---\nname: pinrail\n"));
        assert!(!text.contains("{sha}") && !text.contains("{base}"));
    }

    #[test]
    fn the_skill_to_copy_names_the_commands_for_its_references() {
        let text = none().to_copy();
        assert!(
            text.contains("- `pinrail docs building`: Make a plugin"),
            "{text}"
        );
        assert!(!text.contains("references/"), "{text}");
    }

    #[test]
    fn the_skill_carries_its_building_guide_with_the_sdk_version_in_place() {
        let paths: Vec<_> = FILES.iter().map(|(p, _)| *p).collect();
        assert_eq!(paths, ["SKILL.md", "references/building.md"]);
        // the build fills in every placeholder, the SDK's version among them
        for (path, text) in FILES {
            assert!(!text.contains("{{"), "{path}: a placeholder left");
        }
        let building = FILES
            .iter()
            .find(|(p, _)| *p == "references/building.md")
            .unwrap()
            .1;
        assert!(building.contains("npx pinrail-sdk@1."), "{building}");
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

    /// The list plugin, without a skill, and the change map with one, in a
    /// folder under `root`.
    fn plugins(root: &Path) -> Vec<InstalledPlugin> {
        let skill = root.join("change-map/skill");
        fs::create_dir_all(skill.join("scripts")).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\ndescription: Map a change\n---\n# Change map\n",
        )
        .unwrap();
        fs::write(skill.join("scripts/files.mjs"), "// the diff's files\n").unwrap();
        vec![
            InstalledPlugin {
                name: "list".into(),
                title: "List".into(),
                use_when: Some("Items to accept or reject.".into()),
                skill: None,
            },
            InstalledPlugin {
                name: "change-map".into(),
                title: "Change map".into(),
                use_when: Some("A code change to review as a diagram.".into()),
                skill: Some((skill, vec!["SKILL.md".into(), "scripts/files.mjs".into()])),
            },
        ]
    }

    #[test]
    fn the_skill_names_the_plugins_with_a_skill_and_carries_their_skills() {
        let root = tempfile::tempdir().unwrap();
        let skill = Skill::with(&plugins(root.path()));
        let text = skill.text();
        assert!(
            text.contains(
                "derive the payload from the work.\n\nThe installed plugins with a skill:\n\n\
                 - `change-map`: [its skill](plugins/change-map/SKILL.md)\n"
            ),
            "{text}"
        );
        // the others are for `pinrail plugins` to list
        assert!(!text.contains("`list`: "), "{text}");
        assert!(!text.contains("Items to accept or reject."), "{text}");
        assert!(!text.contains("{plugin_skills}"), "{text}");
        assert_ne!(skill.sha, none().sha, "the hash covers the plugins");

        let home = home_with(&[".claude"]);
        connect(home.path(), "claude", &skill).unwrap();
        let dir = home.path().join(".claude/skills/pinrail");
        assert_eq!(
            fs::read_to_string(dir.join("plugins/change-map/scripts/files.mjs")).unwrap(),
            "// the diff's files\n"
        );
        assert_eq!(fs::read_to_string(dir.join("SKILL.md")).unwrap(), text);
        assert_eq!(status(home.path(), &skill)[0].state, State::Connected);
    }

    #[test]
    fn the_skill_to_copy_names_the_command_for_each_plugins_skill() {
        let root = tempfile::tempdir().unwrap();
        let text = Skill::with(&plugins(root.path())).to_copy();
        assert!(
            text.contains("- `change-map`: `pinrail plugins skill change-map`\n"),
            "{text}"
        );
        assert!(!text.contains("](plugins/"), "{text}");
    }

    #[test]
    fn a_removed_plugins_skill_goes_and_the_agents_follow_the_plugins() {
        let root = tempfile::tempdir().unwrap();
        let with = Skill::with(&plugins(root.path()));
        let home = home_with(&[".claude", ".codex"]);
        connect(home.path(), "claude", &with).unwrap();
        connect(home.path(), "codex", &with).unwrap();
        // Codex's skill as an older app wrote it: its base differs
        let codex = home.path().join(".codex/skills/pinrail/SKILL.md");
        let older = fs::read_to_string(&codex)
            .unwrap()
            .replace(BASE, "0123")
            .replace(&with.sha, "4567");
        fs::write(&codex, &older).unwrap();

        // the change map removed: the list alone
        let without = Skill::with(&plugins(root.path())[..1]);
        assert_eq!(status(home.path(), &without)[0].state, State::Outdated);
        assert_eq!(refresh_plugins(home.path(), &without), ["Claude Code"]);
        let claude = home.path().join(".claude/skills/pinrail");
        assert!(!claude.join("plugins/change-map").exists());
        assert_eq!(
            fs::read_to_string(claude.join("SKILL.md")).unwrap(),
            without.text()
        );
        // an older app's skill waits for the update the person chooses
        assert_eq!(fs::read_to_string(&codex).unwrap(), older);
        assert_eq!(update_outdated(home.path(), &without), ["Codex"]);
    }

    #[test]
    fn without_plugin_skills_the_skill_names_no_plugins() {
        let root = tempfile::tempdir().unwrap();
        let text = Skill::with(&plugins(root.path())[..1]).text();
        assert!(
            text.contains(
                "it says how to\nderive the payload from the work.\n\nA plugin fits only"
            ),
            "{text}"
        );
        assert!(
            !text.contains("The installed plugins with a skill"),
            "{text}"
        );
        assert!(!text.contains("{plugin_skills}"), "{text}");
        assert_eq!(text, none().text());
    }

    #[test]
    fn a_plugin_skill_that_cannot_be_read_is_left_out_with_its_plugin() {
        let root = tempfile::tempdir().unwrap();
        let mut listed = plugins(root.path());
        fs::remove_file(root.path().join("change-map/skill/scripts/files.mjs")).unwrap();
        // and a path that leaves the skill's folder is never read
        listed[0].skill = Some((
            root.path().join("change-map/skill"),
            vec!["../../secret".into()],
        ));
        let skill = Skill::with(&listed);
        let text = skill.text();
        assert!(!text.contains("`change-map`"), "{text}");
        assert!(!text.contains("plugins/"), "{text}");
        assert!(
            skill
                .files
                .iter()
                .all(|(rel, _)| !rel.starts_with("plugins/"))
        );
    }

    #[test]
    fn an_agent_is_found_by_its_configuration_folder() {
        let home = home_with(&[".claude", ".config/opencode"]);
        let found: Vec<_> = status(home.path(), &none())
            .into_iter()
            .filter(|a| a.found)
            .map(|a| a.id)
            .collect();
        assert_eq!(found, vec!["claude", "opencode"]);
        let claude = state(home.path(), "claude");
        assert_eq!(claude.state, State::Absent);
        assert!(
            Path::new(&claude.skill).ends_with(".claude/skills/pinrail/SKILL.md"),
            "{}",
            claude.skill
        );
    }

    #[test]
    fn connecting_writes_the_skill_and_removing_takes_it_away() {
        let home = home_with(&[".codex"]);
        connect(home.path(), "codex", &none()).unwrap();
        let path = home.path().join(".codex/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(&path).unwrap(), none().text());
        let building = home
            .path()
            .join(".codex/skills/pinrail/references/building.md");
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
        let refused = connect(home.path(), "antigravity", &none()).unwrap_err();
        assert!(
            refused.contains("Antigravity CLI is not installed"),
            "{refused}"
        );
        assert!(!home.path().join(".gemini").exists());
    }

    #[test]
    fn a_skill_with_other_contents_is_outdated_and_connecting_updates_it() {
        let home = home_with(&[".claude"]);
        connect(home.path(), "claude", &none()).unwrap();
        let dir = home.path().join(".claude/skills/pinrail");
        // as an older app wrote it: another hash, and a reference since dropped
        let older = none().text().replace(&none().sha, "0123");
        fs::write(dir.join("SKILL.md"), &older).unwrap();
        fs::write(dir.join("references/dropped.md"), "# Dropped\n").unwrap();
        assert_eq!(state(home.path(), "claude").state, State::Outdated);

        connect(home.path(), "claude", &none()).unwrap();
        assert_eq!(state(home.path(), "claude").state, State::Connected);
        assert_eq!(
            fs::read_to_string(dir.join("SKILL.md")).unwrap(),
            none().text()
        );
        assert!(!dir.join("references/dropped.md").exists());
    }

    #[test]
    fn updating_outdated_skills_leaves_the_others_alone() {
        let home = home_with(&[".claude", ".codex", ".cursor", ".config/opencode"]);
        connect(home.path(), "claude", &none()).unwrap();
        connect(home.path(), "codex", &none()).unwrap();
        // Claude Code's skill as an older app wrote it
        let claude = home.path().join(".claude/skills/pinrail/SKILL.md");
        fs::write(&claude, none().text().replace(&none().sha, "0123")).unwrap();
        // a skill of the person's own
        let cursor = home.path().join(".cursor/skills/pinrail");
        fs::create_dir_all(&cursor).unwrap();
        fs::write(cursor.join("SKILL.md"), "---\nname: pinrail\n---\nmine\n").unwrap();

        assert_eq!(update_outdated(home.path(), &none()), ["Claude Code"]);
        assert_eq!(fs::read_to_string(&claude).unwrap(), none().text());
        assert_eq!(state(home.path(), "codex").state, State::Connected);
        assert_eq!(state(home.path(), "cursor").state, State::Theirs);
        // an agent never connected is not connected by an update
        assert_eq!(state(home.path(), "opencode").state, State::Absent);
    }

    #[test]
    fn only_the_hash_decides_whether_a_skill_is_current() {
        let home = home_with(&[".claude"]);
        connect(home.path(), "claude", &none()).unwrap();
        // a reference changed by hand does not make the skill outdated, nor
        // does any other change that keeps the hash
        let dir = home.path().join(".claude/skills/pinrail");
        fs::write(dir.join("references/building.md"), "# Building\n").unwrap();
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
            connect(home.path(), "cursor", &none())
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
        connect(home.path(), "claude", &none()).unwrap();
        let dir = home.path().join(".claude/skills/pinrail");
        fs::write(dir.join("notes.md"), "mine").unwrap();
        disconnect(home.path(), "claude").unwrap();
        assert!(!dir.join("SKILL.md").exists());
        assert!(!dir.join("references").exists());
        assert!(dir.join("notes.md").exists());
    }

    #[test]
    fn opencode_gets_its_own_skill_beside_claude_codes() {
        let home = home_with(&[".claude", ".config/opencode"]);
        connect(home.path(), "claude", &none()).unwrap();
        let opencode = state(home.path(), "opencode");
        assert_eq!(opencode.state, State::Absent);
        assert_eq!(opencode.also_reads, ["Claude Code"]);
        connect(home.path(), "opencode", &none()).unwrap();
        let path = home.path().join(".config/opencode/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(path).unwrap(), none().text());
        assert_eq!(state(home.path(), "opencode").state, State::Connected);
        assert_eq!(state(home.path(), "claude").state, State::Connected);
    }

    #[test]
    fn antigravity_keeps_its_skills_in_geminis_config_folder() {
        let home = home_with(&[".gemini"]);
        connect(home.path(), "antigravity", &none()).unwrap();
        let path = home.path().join(".gemini/config/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(path).unwrap(), none().text());
        assert_eq!(state(home.path(), "antigravity").state, State::Connected);
    }

    #[test]
    fn grok_gets_its_own_skill_beside_claude_codes_and_cursors() {
        let home = home_with(&[".grok", ".claude", ".cursor"]);
        connect(home.path(), "claude", &none()).unwrap();
        connect(home.path(), "cursor", &none()).unwrap();
        let grok = state(home.path(), "grok");
        assert!(grok.found);
        assert_eq!(grok.state, State::Absent);
        assert_eq!(grok.also_reads, ["Claude Code", "Cursor"]);
        connect(home.path(), "grok", &none()).unwrap();
        let path = home.path().join(".grok/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(path).unwrap(), none().text());
        assert_eq!(state(home.path(), "grok").state, State::Connected);
    }

    #[test]
    fn goose_gets_its_own_skill_beside_claude_codes() {
        let home = home_with(&[".claude", ".config/goose"]);
        connect(home.path(), "claude", &none()).unwrap();
        let goose = state(home.path(), "goose");
        assert!(goose.found);
        assert_eq!(goose.state, State::Absent);
        assert_eq!(goose.also_reads, ["Claude Code"]);
        connect(home.path(), "goose", &none()).unwrap();
        let path = home.path().join(".config/goose/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(path).unwrap(), none().text());
        assert_eq!(state(home.path(), "goose").state, State::Connected);
    }

    #[test]
    fn amp_gets_its_own_skill_beside_claude_codes() {
        let home = home_with(&[".claude", ".config/amp"]);
        connect(home.path(), "claude", &none()).unwrap();
        let amp = state(home.path(), "amp");
        assert!(amp.found);
        assert_eq!(amp.state, State::Absent);
        assert_eq!(amp.also_reads, ["Claude Code"]);
        connect(home.path(), "amp", &none()).unwrap();
        let path = home.path().join(".config/amp/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(path).unwrap(), none().text());
        assert_eq!(state(home.path(), "amp").state, State::Connected);
    }

    #[test]
    fn copilot_cli_keeps_its_skills_in_its_folder() {
        let home = home_with(&[".copilot"]);
        assert!(state(home.path(), "copilot").found);
        connect(home.path(), "copilot", &none()).unwrap();
        let path = home.path().join(".copilot/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(path).unwrap(), none().text());
        assert_eq!(state(home.path(), "copilot").state, State::Connected);
    }

    #[test]
    fn kiro_cli_keeps_its_skills_in_its_folder() {
        let home = home_with(&[".kiro"]);
        assert!(state(home.path(), "kiro").found);
        connect(home.path(), "kiro", &none()).unwrap();
        let path = home.path().join(".kiro/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(path).unwrap(), none().text());
        assert_eq!(state(home.path(), "kiro").state, State::Connected);
    }

    #[test]
    fn augment_gets_its_own_skill_beside_claude_codes() {
        let home = home_with(&[".claude", ".augment"]);
        connect(home.path(), "claude", &none()).unwrap();
        let augment = state(home.path(), "augment");
        assert!(augment.found);
        assert_eq!(augment.state, State::Absent);
        assert_eq!(augment.also_reads, ["Claude Code"]);
        connect(home.path(), "augment", &none()).unwrap();
        let path = home.path().join(".augment/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(path).unwrap(), none().text());
        assert_eq!(state(home.path(), "augment").state, State::Connected);
    }

    #[test]
    fn cline_keeps_its_skills_in_its_folder() {
        let home = home_with(&[".cline"]);
        assert!(state(home.path(), "cline").found);
        connect(home.path(), "cline", &none()).unwrap();
        let path = home.path().join(".cline/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(path).unwrap(), none().text());
        assert_eq!(state(home.path(), "cline").state, State::Connected);
    }

    #[test]
    fn hermes_agent_keeps_its_skills_in_its_folder() {
        let home = home_with(&[".hermes"]);
        assert!(state(home.path(), "hermes").found);
        connect(home.path(), "hermes", &none()).unwrap();
        let path = home.path().join(".hermes/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(path).unwrap(), none().text());
        assert_eq!(state(home.path(), "hermes").state, State::Connected);
    }

    #[test]
    fn pi_keeps_its_skills_in_its_agent_folder() {
        let home = home_with(&[".pi/agent"]);
        assert!(state(home.path(), "pi").found);
        connect(home.path(), "pi", &none()).unwrap();
        let path = home.path().join(".pi/agent/skills/pinrail/SKILL.md");
        assert_eq!(fs::read_to_string(path).unwrap(), none().text());
        assert_eq!(state(home.path(), "pi").state, State::Connected);
    }
}
