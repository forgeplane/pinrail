//! Pinrail's hooks in an agent's settings: installed beside the person's
//! own, recognised again, and taken out leaving the file as it was.

use std::path::Path;

use pinrail_runtime::adapter;
use pinrail_runtime::integration::{
    HookPlan, HookState, apply, file_state, install, remove, state,
};
use serde_json::{Value, json};

fn plan() -> HookPlan {
    adapter("claude-code")
        .unwrap()
        .hook_plan(Path::new("/Users/you"), &|_| None)
        .unwrap()
}

fn own_hooks() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/settings/claude-code-own-hooks.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn claude_code_s_hooks_go_in_its_settings_file() {
    let plan = plan();
    assert_eq!(plan.file, Path::new("/Users/you/.claude/settings.json"));
    let moved = adapter("claude-code")
        .unwrap()
        .hook_plan(Path::new("/Users/you"), &|k| {
            (k == "CLAUDE_CONFIG_DIR").then(|| "/work/claude".to_string())
        })
        .unwrap();
    assert_eq!(moved.file, Path::new("/work/claude/settings.json"));
}

#[test]
fn installing_keeps_the_person_s_settings_and_removing_restores_them() {
    let plan = plan();
    let original = own_hooks();
    assert_eq!(state(Some(&original), &plan), HookState::Absent);
    let mut settings = original.clone();
    install(&mut settings, &plan);
    assert_eq!(state(Some(&settings), &plan), HookState::Current);
    // the person's own hooks and settings are still there
    assert_eq!(settings["model"], "opus");
    assert_eq!(settings["permissions"], original["permissions"]);
    assert_eq!(settings["hooks"]["Stop"][0], original["hooks"]["Stop"][0]);
    assert_eq!(
        settings["hooks"]["PostToolUse"][0],
        original["hooks"]["PostToolUse"][0]
    );
    // once more changes nothing
    let mut again = settings.clone();
    install(&mut again, &plan);
    assert_eq!(again, settings);
    remove(&mut settings);
    assert_eq!(settings, original);
}

#[test]
fn an_empty_file_gets_one_hook_for_each_event_and_loses_them_again() {
    let plan = plan();
    let mut settings = json!({});
    install(&mut settings, &plan);
    let stop = &settings["hooks"]["Stop"][0]["hooks"][0];
    assert_eq!(
        stop["command"],
        "\"${PINRAIL_CLI:-pinrail}\" hook claude-code stop"
    );
    assert_eq!(stop["timeout"], 2);
    assert_eq!(settings["hooks"]["PermissionRequest"][0]["matcher"], "*");
    remove(&mut settings);
    assert_eq!(settings, json!({}));
}

#[test]
fn hooks_from_another_version_or_edited_by_hand_are_told_apart() {
    let plan = plan();
    let mut older = json!({});
    let fewer = HookPlan {
        file: plan.file.clone(),
        entries: plan.entries[..3].to_vec(),
    };
    install(&mut older, &fewer);
    assert_eq!(state(Some(&older), &plan), HookState::Outdated);
    let mut edited = json!({});
    install(&mut edited, &plan);
    edited["hooks"]["Stop"][0]["hooks"][0]["timeout"] = json!(30);
    assert_eq!(state(Some(&edited), &plan), HookState::Modified);
}

#[test]
fn a_file_pinrail_cannot_edit_safely_is_unreadable() {
    let plan = plan();
    for settings in [
        None,
        Some(json!([])),
        Some(json!({"hooks": "none"})),
        Some(json!({"hooks": {"Stop": {"command": "x"}}})),
    ] {
        assert_eq!(
            state(settings.as_ref(), &plan),
            HookState::Unreadable,
            "{settings:?}"
        );
    }
}

#[test]
fn every_hook_installed_is_one_pinrail_hook_reads() {
    let known = [
        "session-start",
        "prompt",
        "permission",
        "permission-denied",
        "notification",
        "tool-done",
        "stop",
        "stop-failure",
        "session-end",
    ];
    for entry in plan().entries {
        let hook = entry.command.rsplit(' ').next().unwrap();
        assert!(known.contains(&hook), "{}", entry.command);
        assert!(entry.command.contains(" hook claude-code "));
    }
}

#[test]
fn installing_writes_the_file_in_its_own_order_and_keeps_a_backup_once() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    let original = "{\n  \"permissions\": {},\n  \"model\": \"opus\"\n}\n";
    std::fs::write(&file, original).unwrap();
    let plan = HookPlan {
        file: file.clone(),
        ..plan()
    };
    assert_eq!(apply(&plan, true).unwrap(), HookState::Current);
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(
        written.find("permissions").unwrap() < written.find("model").unwrap(),
        "{written}"
    );
    let backup = dir.path().join("settings.json.pinrail-backup");
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);
    assert_eq!(file_state(&plan), HookState::Current);
    // the backup stays the person's original
    assert_eq!(apply(&plan, false).unwrap(), HookState::Absent);
    apply(&plan, true).unwrap();
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);
    let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(entries.len(), 2, "no temporary file is left");
}

#[test]
fn a_file_that_does_not_parse_is_never_written() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    std::fs::write(&file, "{ \"model\": \"opus\", }").unwrap();
    let plan = HookPlan {
        file: file.clone(),
        ..plan()
    };
    assert_eq!(file_state(&plan), HookState::Unreadable);
    assert!(apply(&plan, true).is_err());
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "{ \"model\": \"opus\", }"
    );
    assert!(!dir.path().join("settings.json.pinrail-backup").exists());
}

#[test]
fn a_missing_file_is_created_with_the_hooks() {
    let dir = tempfile::tempdir().unwrap();
    let plan = HookPlan {
        file: dir.path().join("claude").join("settings.json"),
        ..plan()
    };
    assert_eq!(file_state(&plan), HookState::Absent);
    assert_eq!(apply(&plan, true).unwrap(), HookState::Current);
    assert_eq!(apply(&plan, false).unwrap(), HookState::Absent);
}
