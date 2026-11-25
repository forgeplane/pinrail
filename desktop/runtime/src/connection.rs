//! Connecting an agent to Pinrail: the `pinrail` skill in the agent's
//! skills folder, as `<skills>/pinrail/` with SKILL.md and the folders the
//! app writes beside it, and the files of Pinrail's own that go with it.
//! The `Agent` trait connects and removes an agent through these, and an
//! adapter changes only what differs for its agent.
//!
//! A skill the app wrote says `managed-by: Pinrail` in its front matter, so
//! only the app's own is updated or removed; a `pinrail` skill without it
//! is the person's. The front matter also carries `sha`, the SHA-256 of
//! everything written, which tells a current skill from an outdated one.

use std::fs;
use std::path::{Path, PathBuf};

use crate::registry::Folders;

/// The line of the front matter that marks the skill as the app's.
pub const MARK: &str = "managed-by: Pinrail";
/// The folders the app writes beside SKILL.md: the skill's references and
/// the skills of the installed plugins. They are all the app's.
pub const REFERENCES: &str = "references";
pub const PLUGINS: &str = "plugins";

/// The skill as the app writes it: each file's path in the skill's folder,
/// such as `references/building.md`, with its bytes, and the SHA-256 that
/// SKILL.md's front matter gives for them.
#[derive(Debug, Clone, Copy)]
pub struct SkillFiles<'a> {
    pub files: &'a [(String, Vec<u8>)],
    pub sha: &'a str,
}

/// What is in an agent's place for the skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillState {
    /// no `pinrail` skill
    Absent,
    /// the app's skill, as the app writes it now, with its files
    Connected,
    /// the app's skill, with other contents or without a file that goes
    /// with it
    Outdated,
    /// a `pinrail` skill that is not the app's
    Theirs,
}

/// Makes the command that runs a command line in the person's login shell,
/// which finds the agent's program as a terminal would.
pub type Shell<'a> = &'a dyn Fn(&str) -> std::process::Command;

/// Where an agent with `folders` keeps the skill's SKILL.md under `home`.
pub fn skill_path(home: &Path, folders: &Folders) -> PathBuf {
    home.join(folders.skills).join("pinrail").join("SKILL.md")
}

/// The state of the skill at `path`, against the skill with `sha`.
pub fn state_of(path: &Path, sha: &str) -> SkillState {
    match fs::read_to_string(path) {
        Err(_) => SkillState::Absent,
        Ok(text) if !managed(&text) => SkillState::Theirs,
        Ok(text) if front_matter_value(&text, "sha") == Some(sha) => SkillState::Connected,
        Ok(_) => SkillState::Outdated,
    }
}

/// Whether the front matter marks the skill as the app's.
pub fn managed(text: &str) -> bool {
    front_matter(text).is_some_and(|mut lines| lines.any(|line| line == MARK))
}

/// The value the front matter gives for `key`, such as `sha`.
pub fn front_matter_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let key = format!("{key}:");
    front_matter(text)?.find_map(|line| line.strip_prefix(key.as_str()).map(str::trim))
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

/// Writes the skill's files into the folder of `path`, SKILL.md last, so
/// that the skill counts as there only once it is whole. The folders the
/// app wrote before go first, so a reference or a plugin dropped since is
/// left out.
pub(crate) fn write_skill(path: &Path, skill: &SkillFiles) -> Result<(), String> {
    let dir = path.parent().expect("the skill's path has a folder");
    remove_generated(dir)?;
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

/// Removes the app's skill at `path`, the folders it wrote beside it, and
/// its folder when nothing else is in it. A skill of the person's own
/// stays, and is an error.
pub(crate) fn remove_skill(path: &Path) -> Result<(), String> {
    match fs::read_to_string(path) {
        Err(_) => return Ok(()),
        Ok(text) if !managed(&text) => return Err(theirs(path)),
        Ok(_) => {}
    }
    fs::remove_file(path).map_err(|e| format!("removing {}: {e}", path.display()))?;
    if let Some(dir) = path.parent() {
        remove_generated(dir)?;
        // only when empty: the person may keep files of their own beside it
        let _ = fs::remove_dir(dir);
    }
    Ok(())
}

/// Why the app leaves the skill at `path` alone.
pub(crate) fn theirs(path: &Path) -> String {
    format!(
        "{} is a skill of yours, not Pinrail's; it is left alone",
        path.display()
    )
}

/// Writes one file beside where it goes and renames it, so an agent never
/// reads half of it.
fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let folder = path.parent().expect("a skill file has a folder");
    fs::create_dir_all(folder).map_err(|e| format!("creating {}: {e}", folder.display()))?;
    let name = path.file_name().unwrap().to_string_lossy();
    let partial = folder.join(format!(".{name}.partial"));
    fs::write(&partial, bytes).map_err(|e| format!("writing {}: {e}", partial.display()))?;
    fs::rename(&partial, path).map_err(|e| format!("writing {}: {e}", path.display()))
}

/// Removes the folders the app writes beside SKILL.md.
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

/// What the adapters' tests share: a home folder of the test's own, a
/// skill as the app writes it, the checks of what an agent has of
/// Pinrail's, and the whole connect and remove of one agent.
#[cfg(test)]
pub(crate) mod testing {
    use std::fs;
    use std::path::Path;

    use serde_json::Value;

    use super::{Shell, SkillFiles, SkillState};
    use crate::agent::Agent;
    use crate::integration::{HookState, Layout, file_state};

    /// No variables: every folder is under the test's home.
    pub fn env(_: &str) -> Option<String> {
        None
    }

    /// A home folder with these folders in it.
    pub fn home_with(folders: &[&str]) -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        for folder in folders {
            fs::create_dir_all(home.path().join(folder)).unwrap();
        }
        home
    }

    /// A skill as the app writes it, with `sha` in its front matter.
    pub fn skill(sha: &str) -> Vec<(String, Vec<u8>)> {
        vec![
            (
                "SKILL.md".to_string(),
                format!(
                    "---\nname: pinrail\nmetadata:\n  managed-by: Pinrail\n  sha: {sha}\n---\nUse Pinrail.\n"
                )
                .into_bytes(),
            ),
            ("references/building.md".to_string(), b"# Building\n".to_vec()),
        ]
    }

    /// The skill `skill("abc")` makes, as the tests connect it.
    pub const SHA: &str = "abc";

    /// A shell for an agent with no trust step: running it fails.
    pub fn no_shell(_: &str) -> std::process::Command {
        std::process::Command::new("false")
    }

    /// Connects `agent` with `skill(SHA)`.
    pub fn connect(agent: &dyn Agent, home: &Path, shell: Shell) -> Result<(), String> {
        let files = skill(SHA);
        let written = SkillFiles {
            files: &files,
            sha: SHA,
        };
        agent.connect(home, &env, &written, shell)
    }

    /// The agent has the skill `connect` wrote, as a whole, and every file
    /// of Pinrail's own that goes with it.
    pub fn assert_skill_present(agent: &dyn Agent, home: &Path) {
        let id = agent.spec().id;
        let path = agent.skill_path(home).unwrap();
        for (rel, bytes) in skill(SHA) {
            let file = path.with_file_name(&rel);
            assert_eq!(
                fs::read(&file).ok(),
                Some(bytes),
                "{id}: {}",
                file.display()
            );
        }
        assert_eq!(
            agent.skill_state(home, &env, SHA),
            SkillState::Connected,
            "{id}"
        );
        for file in agent.skill_files(home, &env) {
            assert_eq!(
                file.state(),
                HookState::Current,
                "{id}: {}",
                file.path.display()
            );
            assert_eq!(fs::read_to_string(&file.path).unwrap(), file.text, "{id}");
        }
    }

    /// The agent has none of the skill, its folder, or the files that go
    /// with it; its skills folder stays.
    pub fn assert_skill_absent(agent: &dyn Agent, home: &Path) {
        let id = agent.spec().id;
        let path = agent.skill_path(home).unwrap();
        let folder = path.parent().unwrap();
        assert!(!folder.exists(), "{id}: {}", folder.display());
        assert!(
            folder.parent().unwrap().is_dir(),
            "{id}: its skills folder stays"
        );
        assert_eq!(
            agent.skill_state(home, &env, SHA),
            SkillState::Absent,
            "{id}"
        );
        for file in agent.skill_files(home, &env) {
            assert!(!file.path.exists(), "{id}: {}", file.path.display());
        }
    }

    /// The agent's file has what connecting adds for its status: each of
    /// Pinrail's hooks under its event, or Pinrail's own file and the entry
    /// that loads it.
    pub fn assert_reporting_present(agent: &dyn Agent, home: &Path) {
        let id = agent.spec().id;
        let plan = agent.integration(home, &env).unwrap();
        assert_eq!(file_state(&plan), HookState::Current, "{id}");
        match plan.layout {
            Layout::File { text, .. } => {
                assert_eq!(fs::read_to_string(&plan.file).unwrap(), text, "{id}");
            }
            Layout::Listed {
                key,
                entry,
                path,
                text,
                ..
            } => {
                let folder = plan.file.parent().unwrap();
                assert_eq!(fs::read_to_string(folder.join(path)).unwrap(), text, "{id}");
                let listed = json(&plan.file);
                assert!(
                    listed[key].as_array().unwrap().iter().any(|e| e == entry),
                    "{id}: {listed}"
                );
            }
            Layout::Settings | Layout::Named(_) | Layout::Flat => {
                let hooks = json(&plan.file);
                assert!(!plan.entries.is_empty(), "{id}");
                for entry in &plan.entries {
                    let under = values_of(&hooks, entry.event);
                    assert!(
                        under
                            .iter()
                            .any(|v| strings(v).contains(&entry.command.as_str())),
                        "{id}: {} does not run {} in {hooks}",
                        entry.event,
                        entry.command
                    );
                }
            }
        }
    }

    /// Nothing of what connecting adds for its status is left: no hook runs Pinrail,
    /// and Pinrail's own file is gone.
    pub fn assert_reporting_absent(agent: &dyn Agent, home: &Path) {
        let id = agent.spec().id;
        let plan = agent.integration(home, &env).unwrap();
        assert_eq!(file_state(&plan), HookState::Absent, "{id}");
        match plan.layout {
            Layout::File { .. } => assert!(!plan.file.exists(), "{id}"),
            Layout::Listed { entry, path, .. } => {
                assert!(!plan.file.parent().unwrap().join(path).exists(), "{id}");
                if plan.file.exists() {
                    assert!(
                        !fs::read_to_string(&plan.file).unwrap().contains(entry),
                        "{id}"
                    );
                }
            }
            Layout::Settings | Layout::Named(_) | Layout::Flat => {
                if plan.file.exists() {
                    let text = fs::read_to_string(&plan.file).unwrap();
                    assert!(!text.contains("pinrail"), "{id}: {text}");
                }
            }
        }
    }

    /// One agent's case: its configuration folder, where its skill goes,
    /// the file its hooks or extension go in, and what the person already has
    /// in that file, which must outlast Pinrail's.
    pub struct Case<'a> {
        pub config: &'a str,
        pub skills: &'a str,
        pub reporting: Option<&'a str>,
        pub theirs: Option<&'a str>,
    }

    /// Connects `agent`, connects it again as the update at start does,
    /// then removes it, and checks each step: the skill, the files that go
    /// with it, the hooks or extension, and the person's own entries, kept
    /// throughout. `shell` runs the agent's program for its trust step.
    pub fn connects_and_removes(agent: &dyn Agent, case: &Case, shell: Shell) {
        let id = agent.spec().id;
        let home = home_with(&[case.config]);
        let home = home.path();
        assert!(agent.found(home), "{id}");
        assert_eq!(
            agent.skill_path(home),
            Some(home.join(case.skills).join("pinrail/SKILL.md")),
            "{id}"
        );
        let plan = agent.integration(home, &env);
        assert_eq!(
            plan.as_ref().map(|p| p.file.clone()),
            case.reporting.map(|file| home.join(file)),
            "{id}"
        );
        if let (Some(plan), Some(theirs)) = (&plan, case.theirs) {
            fs::create_dir_all(plan.file.parent().unwrap()).unwrap();
            fs::write(&plan.file, theirs).unwrap();
        }
        let present = || {
            assert_skill_present(agent, home);
            if plan.is_some() {
                assert_reporting_present(agent, home);
            }
            kept(id, plan.as_ref().map(|p| p.file.as_path()), case.theirs);
        };

        connect(agent, home, shell).unwrap();
        present();
        // connecting again, as the update at start does, changes nothing
        connect(agent, home, shell).unwrap();
        present();

        agent.disconnect(home, &env).unwrap();
        assert_skill_absent(agent, home);
        if plan.is_some() {
            assert_reporting_absent(agent, home);
        }
        kept(id, plan.as_ref().map(|p| p.file.as_path()), case.theirs);
    }

    /// Every string of the person's file `theirs` is still in `file`.
    fn kept(id: &str, file: Option<&Path>, theirs: Option<&str>) {
        if let (Some(file), Some(theirs)) = (file, theirs) {
            let now = json(file);
            let before: Value = serde_json::from_str(theirs).unwrap();
            for text in strings(&before) {
                assert!(
                    strings(&now).contains(&text),
                    "{id}: lost {text} from {now}"
                );
            }
        }
    }

    fn json(path: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    /// The values under every key named `key`, at any depth.
    fn values_of<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
        match value {
            Value::Object(map) => map
                .iter()
                .flat_map(|(k, v)| {
                    let mut found = if k == key { vec![v] } else { Vec::new() };
                    found.extend(values_of(v, key));
                    found
                })
                .collect(),
            Value::Array(list) => list.iter().flat_map(|v| values_of(v, key)).collect(),
            _ => Vec::new(),
        }
    }

    /// Every string in `value`, at any depth.
    fn strings(value: &Value) -> Vec<&str> {
        match value {
            Value::String(text) => vec![text.as_str()],
            Value::Object(map) => map.values().flat_map(strings).collect(),
            Value::Array(list) => list.iter().flat_map(strings).collect(),
            _ => Vec::new(),
        }
    }
}
