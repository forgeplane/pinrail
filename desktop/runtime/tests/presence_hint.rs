//! What an agent's own status file says of its session, read as a hint.

use std::path::{Path, PathBuf};

use pinrail_runtime::{Hint, HintStatus, adapter};

const SESSION: &str = "25322e8a-eaa0-4f8b-94e6-a79dc9f7752b";

/// A home folder of its own, with Claude Code's status file for `pid`
/// holding `text`.
fn home_with(name: &str, pid: u32, text: Option<&str>) -> PathBuf {
    let home = std::env::temp_dir().join(format!("pinrail-hint-{name}-{}", std::process::id()));
    let sessions = home.join(".claude/sessions");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&sessions).unwrap();
    if let Some(text) = text {
        std::fs::write(sessions.join(format!("{pid}.json")), text).unwrap();
    }
    home
}

fn fixture() -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/status/claude-code-idle.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn hint(home: &Path, pid: u32, session: &str) -> Option<Hint> {
    adapter("claude-code")
        .unwrap()
        .presence_hint(home, &|_| None, pid, session)
}

#[test]
fn claude_code_s_status_file_says_its_session_is_idle_after_an_interrupt() {
    let home = home_with("idle", 3044, Some(&fixture()));
    assert_eq!(
        hint(&home, 3044, SESSION),
        Some(Hint {
            status: HintStatus::Idle,
            since_ms: 1794578438785
        })
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_status_file_gives_no_hint_unless_it_is_about_the_session() {
    // another session in the same process, no file, a file that is not JSON
    // and a status Pinrail does not know
    let other = home_with("other", 3044, Some(&fixture()));
    assert_eq!(hint(&other, 3044, "another-session"), None);
    let none = home_with("none", 3044, None);
    assert_eq!(hint(&none, 3044, SESSION), None);
    let broken = home_with("broken", 3044, Some("{not json"));
    assert_eq!(hint(&broken, 3044, SESSION), None);
    let odd = fixture().replace("\"idle\"", "\"thinking\"");
    let odd = home_with("odd", 3044, Some(&odd));
    assert_eq!(hint(&odd, 3044, SESSION), None);
    for home in [other, none, broken, odd] {
        let _ = std::fs::remove_dir_all(home);
    }
}

#[test]
fn claude_code_s_moved_configuration_folder_is_where_its_status_files_are() {
    let moved = home_with("moved", 3044, Some(&fixture())).join(".claude");
    let elsewhere = std::env::temp_dir().join("pinrail-hint-nowhere");
    let config = moved.display().to_string();
    let found = adapter("claude-code").unwrap().presence_hint(
        &elsewhere,
        &|k| (k == "CLAUDE_CONFIG_DIR").then(|| config.clone()),
        3044,
        SESSION,
    );
    assert_eq!(found.map(|h| h.status), Some(HintStatus::Idle));
    // agents without a status file give none
    assert_eq!(
        adapter("codex")
            .unwrap()
            .presence_hint(&moved, &|_| None, 3044, SESSION),
        None
    );
    let _ = std::fs::remove_dir_all(moved.parent().unwrap());
}
