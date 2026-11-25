//! Connecting an agent to Pinrail and removing it again, as every agent
//! does it: what counts as the app's skill, and what is left alone. Each
//! adapter's own tests check where its agent keeps them.

use std::fs;
use std::path::Path;

use pinrail_runtime::connection::{SkillFiles, SkillState, managed};
use pinrail_runtime::integration::HookState;
use pinrail_runtime::{Agent, connector};

/// A shell for agents with no trust step: running it fails.
fn no_shell(_: &str) -> std::process::Command {
    std::process::Command::new("false")
}

/// No variables: every folder is under the test's home.
fn env(_: &str) -> Option<String> {
    None
}

/// A home folder with these folders in it.
fn home_with(folders: &[&str]) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    for folder in folders {
        fs::create_dir_all(home.path().join(folder)).unwrap();
    }
    home
}

/// A skill as the app writes it, with `sha` in its front matter.
fn skill(sha: &str) -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "SKILL.md".to_string(),
            format!("---\nname: pinrail\nmetadata:\n  managed-by: Pinrail\n  sha: {sha}\n---\nUse Pinrail.\n")
                .into_bytes(),
        ),
        ("references/building.md".to_string(), b"# Building\n".to_vec()),
    ]
}

fn agent(id: &str) -> &'static dyn Agent {
    connector(id).unwrap()
}

fn state(home: &Path, id: &str, sha: &str) -> SkillState {
    agent(id).skill_state(home, &env, sha)
}

#[test]
fn an_agent_that_is_not_installed_is_not_connected() {
    let home = home_with(&[]);
    let files = skill("abc");
    let skill = SkillFiles {
        files: &files,
        sha: "abc",
    };
    let refused = agent("antigravity")
        .connect(home.path(), &env, &skill, &no_shell)
        .unwrap_err();
    assert!(
        refused.contains("Antigravity CLI is not installed"),
        "{refused}"
    );
    assert!(!home.path().join(".gemini").exists());
}

#[test]
fn a_skill_with_another_hash_is_outdated_and_connecting_updates_it() {
    let home = home_with(&[".claude"]);
    let claude = agent("claude-code");
    let older = skill("0123");
    claude
        .connect(
            home.path(),
            &env,
            &SkillFiles {
                files: &older,
                sha: "0123",
            },
            &no_shell,
        )
        .unwrap();
    let dir = home.path().join(".claude/skills/pinrail");
    fs::write(dir.join("references/dropped.md"), "# Dropped\n").unwrap();
    assert_eq!(
        state(home.path(), "claude-code", "abc"),
        SkillState::Outdated
    );
    // a reference changed by hand changes nothing: only the hash counts
    fs::write(dir.join("references/building.md"), "# Mine\n").unwrap();
    assert_eq!(
        state(home.path(), "claude-code", "0123"),
        SkillState::Connected
    );

    let files = skill("abc");
    claude
        .connect(
            home.path(),
            &env,
            &SkillFiles {
                files: &files,
                sha: "abc",
            },
            &no_shell,
        )
        .unwrap();
    assert_eq!(
        state(home.path(), "claude-code", "abc"),
        SkillState::Connected
    );
    assert!(!dir.join("references/dropped.md").exists());
}

#[test]
fn a_pinrail_skill_of_the_person_s_own_stays_and_connecting_adds_only_the_hooks() {
    let home = home_with(&[".cursor"]);
    let dir = home.path().join(".cursor/skills/pinrail");
    fs::create_dir_all(&dir).unwrap();
    let theirs = "---\nname: pinrail\ndescription: mine\n---\n\nMy own rules.\n";
    fs::write(dir.join("SKILL.md"), theirs).unwrap();
    let files = skill("abc");
    let skill = SkillFiles {
        files: &files,
        sha: "abc",
    };
    let cursor = agent("cursor");
    let hooks = || cursor.reporting_state(home.path(), &env).unwrap();
    assert_eq!(state(home.path(), "cursor", "abc"), SkillState::Theirs);
    cursor
        .connect(home.path(), &env, &skill, &no_shell)
        .unwrap();
    assert_eq!(fs::read_to_string(dir.join("SKILL.md")).unwrap(), theirs);
    assert_eq!(hooks(), HookState::Current);

    cursor.disconnect(home.path(), &env).unwrap();
    assert_eq!(fs::read_to_string(dir.join("SKILL.md")).unwrap(), theirs);
    assert_eq!(hooks(), HookState::Absent);
}

#[test]
fn an_agent_without_hooks_is_not_connected_over_a_skill_of_the_person_s_own() {
    let home = home_with(&[".kiro/skills/pinrail"]);
    let path = home.path().join(".kiro/skills/pinrail/SKILL.md");
    fs::write(&path, "---\nname: pinrail\n---\nmine\n").unwrap();
    let files = skill("abc");
    let refused = agent("kiro")
        .connect(
            home.path(),
            &env,
            &SkillFiles {
                files: &files,
                sha: "abc",
            },
            &no_shell,
        )
        .unwrap_err();
    assert!(refused.contains("a skill of yours"), "{refused}");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "---\nname: pinrail\n---\nmine\n"
    );
}

#[test]
fn a_hooks_file_pinrail_cannot_read_is_left_alone_and_the_skill_still_added() {
    let home = home_with(&[".claude"]);
    let settings = home.path().join(".claude/settings.json");
    fs::write(&settings, "{ not json").unwrap();
    let files = skill("abc");
    let claude = agent("claude-code");
    claude
        .connect(
            home.path(),
            &env,
            &SkillFiles {
                files: &files,
                sha: "abc",
            },
            &no_shell,
        )
        .unwrap();
    assert_eq!(
        state(home.path(), "claude-code", "abc"),
        SkillState::Connected
    );
    assert_eq!(
        claude.reporting_state(home.path(), &env),
        Some(HookState::Unreadable)
    );
    claude.disconnect(home.path(), &env).unwrap();
    assert_eq!(fs::read_to_string(&settings).unwrap(), "{ not json");
    assert_eq!(state(home.path(), "claude-code", "abc"), SkillState::Absent);
}

#[test]
fn removing_keeps_files_of_the_person_s_own_beside_the_skill() {
    let home = home_with(&[".claude"]);
    let files = skill("abc");
    let claude = agent("claude-code");
    claude
        .connect(
            home.path(),
            &env,
            &SkillFiles {
                files: &files,
                sha: "abc",
            },
            &no_shell,
        )
        .unwrap();
    let dir = home.path().join(".claude/skills/pinrail");
    fs::write(dir.join("notes.md"), "mine").unwrap();
    claude.disconnect(home.path(), &env).unwrap();
    assert!(!dir.join("SKILL.md").exists());
    assert!(!dir.join("references").exists());
    assert!(dir.join("notes.md").exists());
}

#[test]
fn the_mark_counts_only_in_the_front_matter() {
    assert!(!managed("# Notes\n\nmanaged-by: Pinrail\n"));
    assert!(!managed("---\nname: pinrail\n---\nmanaged-by: Pinrail\n"));
    assert!(managed(
        "---\nname: pinrail\nmetadata:\n  managed-by: Pinrail\n---\nbody\n"
    ));
}
