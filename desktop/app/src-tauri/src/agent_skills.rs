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

use pinrail_runtime::Agent;
use pinrail_runtime::connection::{
    PLUGINS, SkillFiles, SkillState, front_matter_value, skill_path,
};
use pinrail_runtime::skills::skill_text;
use serde::Serialize;
use sha2::{Digest, Sha256};

/// The app's own files of the skill: each one's path in the skill's folder,
/// such as `references/building.md`, and its text, SKILL.md with `{sha}`
/// and `{base}` still in its front matter.
const FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/skill_files.rs"));
/// The SHA-256 of the app's own files.
const BASE: &str = include!(concat!(env!("OUT_DIR"), "/skill_sha.rs"));

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
                let text = skill_text(text);
                let text = if *rel == "SKILL.md" {
                    text.replace("{plugin_skills}", &named)
                } else {
                    text
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

    /// Its files and SHA-256, as an agent's connect writes them.
    pub fn files(&self) -> SkillFiles<'_> {
        SkillFiles {
            files: &self.files,
            sha: &self.sha,
        }
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

/// One agent, as Settings shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentStatus {
    pub id: &'static str,
    pub name: &'static str,
    /// its configuration folder is there
    pub found: bool,
    /// where the skill goes
    pub skill: String,
    /// the connection as a whole: the skill, and the hooks, extension or
    /// plugin for an agent whose status Pinrail reports
    pub state: State,
    /// the skill alone, which is `theirs` when the person's own is kept
    pub skill_state: State,
    /// the other agents whose skill folders this one also reads
    pub also_reads: Vec<&'static str>,
    /// What connecting installs for the agent's status, for an agent
    /// Pinrail can report the status of
    pub integration: Option<Integration>,
}

/// What connecting installs for one agent's status: hooks, an extension
/// or a plugin, the file it goes in, and what the file holds of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Integration {
    pub kind: &'static str,
    pub file: String,
    pub state: &'static str,
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

/// Every agent the app installs the skill for, in the order Settings lists
/// them.
fn agents() -> impl Iterator<Item = &'static dyn Agent> {
    pinrail_runtime::all()
        .iter()
        .filter(|agent| agent.folders.is_some())
        .filter_map(|agent| pinrail_runtime::connector(agent.id))
}

fn find(id: &str) -> Result<&'static dyn Agent, String> {
    agents()
        .find(|agent| agent.spec().id == id)
        .ok_or_else(|| format!("Pinrail does not know an agent called {id}"))
}

/// The environment variable `key`, which can move an agent's folders, such
/// as `XDG_CONFIG_HOME`. A test sees none, so that it never reaches the
/// person's own folders.
fn env(key: &str) -> Option<String> {
    if cfg!(test) {
        return None;
    }
    std::env::var(key).ok()
}

/// Every agent the app knows, found or not.
pub fn status(home: &Path, skill: &Skill) -> Vec<AgentStatus> {
    agents()
        .map(|agent| {
            let spec = agent.spec();
            let folders = spec
                .folders
                .as_ref()
                .expect("an agent that takes the skill");
            AgentStatus {
                id: spec.id,
                name: spec.label,
                found: agent.found(home),
                skill: skill_path(home, folders).display().to_string(),
                state: overall(agent, home, skill),
                skill_state: agent.skill_state(home, &env, &skill.sha).into(),
                also_reads: folders
                    .also_reads
                    .iter()
                    .filter_map(|id| pinrail_runtime::find(id))
                    .map(|a| a.label)
                    .collect(),
                integration: agent.integration(home, &env).map(|plan| Integration {
                    kind: plan.kind.as_str(),
                    file: plan.file.display().to_string(),
                    state: pinrail_runtime::integration::file_state(&plan).as_str(),
                }),
            }
        })
        .collect()
}

/// The state of an agent's connection as a whole: connected once the skill
/// and what reports its status are both as this version writes them,
/// outdated while either is older or missing beside the other, and the
/// person's own while their skill is there and nothing of Pinrail's. A
/// hooks file Pinrail cannot read leaves the skill alone to decide.
fn overall(agent: &dyn Agent, home: &Path, skill: &Skill) -> State {
    use pinrail_runtime::integration::HookState;
    let reporting = agent.reporting_state(home, &env);
    let hooks = matches!(
        reporting,
        Some(HookState::Current | HookState::Outdated | HookState::Modified)
    );
    let hooks_current = !matches!(
        reporting,
        Some(HookState::Absent | HookState::Outdated | HookState::Modified)
    );
    match agent.skill_state(home, &env, &skill.sha) {
        SkillState::Outdated => State::Outdated,
        SkillState::Connected if hooks_current => State::Connected,
        SkillState::Connected => State::Outdated,
        SkillState::Theirs if !hooks => State::Theirs,
        SkillState::Theirs if hooks_current => State::Connected,
        SkillState::Absent if !hooks => State::Absent,
        SkillState::Theirs | SkillState::Absent => State::Outdated,
    }
}

/// A command that runs `line` in the person's login shell, from their home
/// folder, so that it finds the programs a terminal finds.
fn in_login_shell(home: &Path, line: &str) -> std::process::Command {
    #[cfg(not(windows))]
    let mut command = {
        let mut command = std::process::Command::new(crate::cli_install::this_login_shell());
        command.args(["-l", "-c", &format!("exec {line}")]);
        command
    };
    #[cfg(windows)]
    let mut command = {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", line]);
        command
    };
    command.current_dir(home);
    command
}

/// Connects an installed agent, or brings its connection up to date: the
/// skill, and the hooks, extension or plugin that report its status. A
/// `pinrail` skill of the person's own is never replaced.
pub fn connect(home: &Path, id: &str, skill: &Skill) -> Result<(), String> {
    find(id)?.connect(home, &env, &skill.files(), &|line| {
        in_login_shell(home, line)
    })
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
    agents()
        .filter(|agent| agent.found(home))
        .filter(|agent| {
            agent.skill_state(home, &env, &skill.sha) == SkillState::Outdated
                && agent
                    .skill_path(home)
                    .and_then(|path| fs::read_to_string(path).ok())
                    .is_some_and(|text| front_matter_value(&text, "base") == Some(BASE))
        })
        .filter(|agent| {
            agent
                .connect(home, &env, &skill.files(), &|line| {
                    in_login_shell(home, line)
                })
                .is_ok()
        })
        .map(|agent| agent.spec().label)
        .collect()
}

/// Removes what connecting an agent added: the skill, its folder when
/// nothing else is in it, and Pinrail's hooks, extension or plugin. A skill
/// of the person's own stays, and so does a file Pinrail cannot read.
pub fn disconnect(home: &Path, id: &str) -> Result<(), String> {
    find(id)?.disconnect(home, &env)
}

impl From<SkillState> for State {
    fn from(state: SkillState) -> State {
        match state {
            SkillState::Absent => State::Absent,
            SkillState::Connected => State::Connected,
            SkillState::Outdated => State::Outdated,
            SkillState::Theirs => State::Theirs,
        }
    }
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
        assert!(pinrail_runtime::connection::managed(&text));
        assert_eq!(front_matter_value(&text, "sha"), Some(skill.sha.as_str()));
        assert_eq!(front_matter_value(&text, "base"), Some(BASE));
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
        connect(home.path(), "claude-code", &skill).unwrap();
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
        connect(home.path(), "claude-code", &with).unwrap();
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
        assert_eq!(found, vec!["claude-code", "opencode"]);
        let claude = state(home.path(), "claude-code");
        assert_eq!(claude.state, State::Absent);
        assert!(
            Path::new(&claude.skill).ends_with(".claude/skills/pinrail/SKILL.md"),
            "{}",
            claude.skill
        );
    }

    #[test]
    fn updating_outdated_skills_leaves_the_others_alone() {
        let home = home_with(&[".claude", ".codex", ".cursor", ".config/opencode"]);
        connect(home.path(), "claude-code", &none()).unwrap();
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
    fn an_agent_says_whose_skill_folders_it_also_reads() {
        let home = home_with(&[".claude", ".config/opencode", ".grok"]);
        connect(home.path(), "claude-code", &none()).unwrap();
        let opencode = state(home.path(), "opencode");
        assert_eq!(opencode.state, State::Absent);
        assert_eq!(opencode.also_reads, ["Claude Code"]);
        assert_eq!(
            state(home.path(), "grok").also_reads,
            ["Claude Code", "Cursor"]
        );
        connect(home.path(), "opencode", &none()).unwrap();
        assert_eq!(state(home.path(), "opencode").state, State::Connected);
        assert_eq!(state(home.path(), "claude-code").state, State::Connected);
    }

    #[test]
    fn a_connection_counts_as_whole_only_with_its_skill_and_its_hooks() {
        let home = home_with(&[".claude"]);
        connect(home.path(), "claude-code", &none()).unwrap();
        assert_eq!(state(home.path(), "claude-code").state, State::Connected);

        // the skill without the hooks, as an earlier version left an agent
        // whose status was not reported: the update at start adds them
        let settings = home.path().join(".claude/settings.json");
        fs::write(&settings, "{}").unwrap();
        let claude = state(home.path(), "claude-code");
        assert_eq!(
            (claude.state, claude.skill_state),
            (State::Outdated, State::Connected)
        );
        assert_eq!(update_outdated(home.path(), &none()), ["Claude Code"]);
        assert_eq!(state(home.path(), "claude-code").state, State::Connected);
    }

    #[test]
    fn a_skill_of_the_persons_own_with_pinrails_hooks_is_connected() {
        let home = home_with(&[".cursor"]);
        let dir = home.path().join(".cursor/skills/pinrail");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "---\nname: pinrail\n---\nmine\n").unwrap();
        assert_eq!(state(home.path(), "cursor").state, State::Theirs);
        connect(home.path(), "cursor", &none()).unwrap();
        let cursor = state(home.path(), "cursor");
        assert_eq!(
            (cursor.state, cursor.skill_state),
            (State::Connected, State::Theirs)
        );
    }
}
