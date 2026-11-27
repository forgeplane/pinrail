//! Pinrail's hooks in an agent's settings: installed beside the person's
//! own, recognised again, and taken out leaving the file as it was.

use std::path::Path;

use pinrail_runtime::adapter;
use pinrail_runtime::integration::{
    COMMAND_PREFIX, HookState, Integration, apply, file_state, install, remove, state,
};
use serde_json::{Value, json};

fn plan() -> Integration {
    adapter("claude-code")
        .unwrap()
        .integration(Path::new("/Users/you"), &|_| None)
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
        .integration(Path::new("/Users/you"), &|k| {
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
    remove(&mut settings, &plan);
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
        format!("{COMMAND_PREFIX} claude-code stop")
    );
    assert_eq!(stop["timeout"], 2);
    assert_eq!(settings["hooks"]["PermissionRequest"][0]["matcher"], "*");
    remove(&mut settings, &plan);
    assert_eq!(settings, json!({}));
}

#[test]
fn hooks_from_another_version_or_edited_by_hand_are_told_apart() {
    let plan = plan();
    let mut older = json!({});
    let fewer = Integration {
        entries: plan.entries[..3].to_vec(),
        ..plan.clone()
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
    let plan = Integration {
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
    let plan = Integration {
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
    let plan = Integration {
        file: dir.path().join("claude").join("settings.json"),
        ..plan()
    };
    assert_eq!(file_state(&plan), HookState::Absent);
    assert_eq!(apply(&plan, true).unwrap(), HookState::Current);
    assert_eq!(apply(&plan, false).unwrap(), HookState::Absent);
}

#[cfg(unix)]
#[test]
fn a_settings_file_linked_from_a_dotfiles_folder_stays_a_link() {
    let dir = tempfile::tempdir().unwrap();
    let dotfiles = dir.path().join("dotfiles");
    std::fs::create_dir_all(&dotfiles).unwrap();
    std::fs::write(dotfiles.join("settings.json"), "{\"model\": \"opus\"}\n").unwrap();
    let claude = dir.path().join("claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::os::unix::fs::symlink(dotfiles.join("settings.json"), claude.join("settings.json"))
        .unwrap();
    let plan = Integration {
        file: claude.join("settings.json"),
        ..plan()
    };

    assert_eq!(apply(&plan, true).unwrap(), HookState::Current);
    let link = std::fs::symlink_metadata(&plan.file).unwrap();
    assert!(link.file_type().is_symlink(), "the link became a file");
    let target = std::fs::read_to_string(dotfiles.join("settings.json")).unwrap();
    assert!(
        target.contains("hooks") && target.contains("opus"),
        "{target}"
    );
}

fn agy_plan() -> Integration {
    adapter("antigravity")
        .unwrap()
        .integration(Path::new("/Users/you"), &|_| None)
        .unwrap()
}

/// agy's hooks file as herdr leaves it: a named hook of its own.
fn herdr_hook() -> Value {
    json!({
        "herdr": {
            "PreInvocation": [
                {"command": "bash '/Users/you/.gemini/config/hooks/herdr-agent-state.sh' session",
                 "timeout": 10, "type": "command"}
            ]
        }
    })
}

#[test]
fn antigravity_s_hooks_go_in_its_global_hooks_file_as_a_named_hook() {
    let plan = agy_plan();
    assert_eq!(plan.file, Path::new("/Users/you/.gemini/config/hooks.json"));
    let original = herdr_hook();
    assert_eq!(state(Some(&original), &plan), HookState::Absent);
    let mut hooks = original.clone();
    install(&mut hooks, &plan);
    assert_eq!(state(Some(&hooks), &plan), HookState::Current);
    // beside the others, which stay as they were
    assert_eq!(hooks["herdr"], original["herdr"]);
    // most events take their handlers in a list, a tool's event in groups
    assert_eq!(
        hooks["pinrail"]["Stop"][0]["command"],
        format!("{COMMAND_PREFIX} antigravity stop")
    );
    assert_eq!(hooks["pinrail"]["PostToolUse"][0]["matcher"], "*");
    assert_eq!(
        hooks["pinrail"]["PostToolUse"][0]["hooks"][0]["command"],
        format!("{COMMAND_PREFIX} antigravity tool-done")
    );
    let mut again = hooks.clone();
    install(&mut again, &plan);
    assert_eq!(again, hooks);
    remove(&mut hooks, &plan);
    assert_eq!(hooks, original);
}

#[test]
fn antigravity_s_hooks_from_another_version_or_edited_by_hand_are_told_apart() {
    let plan = agy_plan();
    let mut older = json!({});
    let fewer = Integration {
        entries: plan.entries[..2].to_vec(),
        ..plan.clone()
    };
    install(&mut older, &fewer);
    assert_eq!(state(Some(&older), &plan), HookState::Outdated);
    let mut edited = json!({});
    install(&mut edited, &plan);
    edited["pinrail"]["Stop"][0]["timeout"] = json!(30);
    assert_eq!(state(Some(&edited), &plan), HookState::Modified);
    for unreadable in [json!([]), json!({"pinrail": "off"})] {
        assert_eq!(
            state(Some(&unreadable), &plan),
            HookState::Unreadable,
            "{unreadable}"
        );
    }
}

#[test]
fn every_antigravity_hook_installed_is_one_pinrail_hook_reads() {
    for entry in agy_plan().entries {
        let hook = entry.command.rsplit(' ').next().unwrap();
        assert!(
            ["session-start", "invocation", "tool-done", "stop"].contains(&hook),
            "{}",
            entry.command
        );
    }
}

fn grok_plan() -> Integration {
    adapter("grok")
        .unwrap()
        .integration(Path::new("/Users/you"), &|_| None)
        .unwrap()
}

#[test]
fn grok_s_hooks_go_in_a_file_of_pinrail_s_own_in_its_hooks_folder() {
    let plan = grok_plan();
    assert_eq!(plan.file, Path::new("/Users/you/.grok/hooks/pinrail.json"));
    let moved = adapter("grok")
        .unwrap()
        .integration(Path::new("/Users/you"), &|k| {
            (k == "GROK_HOME").then(|| "/work/grok".to_string())
        })
        .unwrap();
    assert_eq!(moved.file, Path::new("/work/grok/hooks/pinrail.json"));
    // Claude Code's format, a group of handlers for each event
    let mut hooks = json!({});
    install(&mut hooks, &plan);
    assert_eq!(state(Some(&hooks), &plan), HookState::Current);
    assert_eq!(
        hooks["hooks"]["StopCancelled"][0]["hooks"][0]["command"],
        format!("{COMMAND_PREFIX} grok stop-cancelled")
    );
    remove(&mut hooks, &plan);
    assert_eq!(state(Some(&hooks), &plan), HookState::Absent);
}

#[test]
fn every_grok_hook_installed_is_one_pinrail_hook_reads() {
    let known = [
        "session-start",
        "prompt",
        "tool",
        "notification",
        "permission-denied",
        "stop",
        "stop-failure",
        "stop-cancelled",
        "session-end",
    ];
    for entry in grok_plan().entries {
        let hook = entry.command.rsplit(' ').next().unwrap();
        assert!(known.contains(&hook), "{}", entry.command);
        assert!(entry.command.contains(" hook grok "));
    }
}

fn copilot_plan() -> Integration {
    adapter("copilot")
        .unwrap()
        .integration(Path::new("/Users/you"), &|_| None)
        .unwrap()
}

#[test]
fn copilot_s_hooks_go_in_a_file_of_pinrail_s_own_listed_by_event() {
    let plan = copilot_plan();
    assert_eq!(
        plan.file,
        Path::new("/Users/you/.copilot/hooks/pinrail.json")
    );
    let moved = adapter("copilot")
        .unwrap()
        .integration(Path::new("/Users/you"), &|k| {
            (k == "COPILOT_HOME").then(|| "/work/copilot".to_string())
        })
        .unwrap();
    assert_eq!(moved.file, Path::new("/work/copilot/hooks/pinrail.json"));
    let mut hooks = json!({});
    install(&mut hooks, &plan);
    assert_eq!(state(Some(&hooks), &plan), HookState::Current);
    // a version, and each event's handlers in a list, without groups
    assert_eq!(hooks["version"], 1);
    assert_eq!(
        hooks["hooks"]["agentStop"][0],
        json!({"type": "command", "command": format!("{COMMAND_PREFIX} copilot stop"), "timeout": 2})
    );
    remove(&mut hooks, &plan);
    assert_eq!(state(Some(&hooks), &plan), HookState::Absent);
}

#[test]
fn a_flat_hooks_file_keeps_the_person_s_handlers() {
    let plan = copilot_plan();
    let original = json!({
        "version": 1,
        "hooks": {"sessionStart": [{"command": "sh herdr.sh session"}]}
    });
    let mut hooks = original.clone();
    install(&mut hooks, &plan);
    assert_eq!(state(Some(&hooks), &plan), HookState::Current);
    assert_eq!(
        hooks["hooks"]["sessionStart"][0],
        original["hooks"]["sessionStart"][0]
    );
    assert_eq!(hooks["hooks"]["sessionStart"].as_array().unwrap().len(), 2);
    let mut again = hooks.clone();
    install(&mut again, &plan);
    assert_eq!(again, hooks);
    remove(&mut hooks, &plan);
    assert_eq!(hooks, original);
    // a handler edited by hand
    install(&mut hooks, &plan);
    hooks["hooks"]["agentStop"][0]["timeout"] = json!(30);
    assert_eq!(state(Some(&hooks), &plan), HookState::Modified);
}

#[test]
fn every_copilot_hook_installed_is_one_pinrail_hook_reads() {
    let known = [
        "session-start",
        "prompt",
        "tool",
        "tool-done",
        "notification",
        "stop",
        "error",
        "session-end",
    ];
    for entry in copilot_plan().entries {
        let hook = entry.command.rsplit(' ').next().unwrap();
        assert!(known.contains(&hook), "{}", entry.command);
    }
}

fn pi_plan() -> Integration {
    adapter("pi")
        .unwrap()
        .integration(Path::new("/Users/you"), &|_| None)
        .unwrap()
}

#[test]
fn pi_s_hooks_are_an_extension_file_of_pinrail_s_own() {
    let plan = pi_plan();
    assert_eq!(
        plan.file,
        Path::new("/Users/you/.pi/agent/extensions/pinrail.ts")
    );
    let moved = adapter("pi")
        .unwrap()
        .integration(Path::new("/Users/you"), &|k| {
            (k == "PI_CODING_AGENT_DIR").then(|| "/work/pi".to_string())
        })
        .unwrap();
    assert_eq!(moved.file, Path::new("/work/pi/extensions/pinrail.ts"));
}

#[test]
fn an_extension_file_is_written_whole_and_removed_whole() {
    let home = tempfile::tempdir().unwrap();
    let plan = adapter("pi")
        .unwrap()
        .integration(home.path(), &|_| None)
        .unwrap();
    assert_eq!(file_state(&plan), HookState::Absent);
    assert_eq!(apply(&plan, true).unwrap(), HookState::Current);
    let text = std::fs::read_to_string(&plan.file).unwrap();
    assert!(text.contains("pinrail hook pi") || text.contains("\"hook\", \"pi\""));
    // another version of it
    std::fs::write(&plan.file, "// Pinrail's extension for Pi, an older one\n").unwrap();
    assert_eq!(file_state(&plan), HookState::Outdated);
    assert_eq!(apply(&plan, true).unwrap(), HookState::Current);
    assert_eq!(apply(&plan, false).unwrap(), HookState::Absent);
    assert!(!plan.file.exists());
    // a file of the person's at that path is never overwritten or removed
    std::fs::write(&plan.file, "export default function () {}\n").unwrap();
    assert_eq!(file_state(&plan), HookState::Unreadable);
    assert!(apply(&plan, true).is_err());
    assert!(apply(&plan, false).is_err());
    assert_eq!(
        std::fs::read_to_string(&plan.file).unwrap(),
        "export default function () {}\n"
    );
}

#[test]
fn cursor_s_hooks_go_beside_the_person_s_in_its_hooks_file() {
    let plan = adapter("cursor")
        .unwrap()
        .integration(Path::new("/Users/you"), &|_| None)
        .unwrap();
    assert_eq!(plan.file, Path::new("/Users/you/.cursor/hooks.json"));
    // the file as herdr leaves it
    let original = json!({
        "version": 1,
        "hooks": {"sessionStart": [{"command": "bash '/Users/you/.cursor/herdr-agent-state.sh' session"}]}
    });
    let mut hooks = original.clone();
    install(&mut hooks, &plan);
    assert_eq!(state(Some(&hooks), &plan), HookState::Current);
    assert_eq!(hooks["hooks"]["sessionStart"].as_array().unwrap().len(), 2);
    assert_eq!(
        hooks["hooks"]["afterAgentResponse"][0]["command"],
        format!("{COMMAND_PREFIX} cursor response")
    );
    remove(&mut hooks, &plan);
    assert_eq!(hooks, original);
    for entry in plan.entries {
        let hook = entry.command.rsplit(' ').next().unwrap();
        assert!(
            [
                "session-start",
                "prompt",
                "tool",
                "response",
                "stop",
                "session-end"
            ]
            .contains(&hook),
            "{}",
            entry.command
        );
    }
}

fn opencode_plan(home: &Path) -> Integration {
    adapter("opencode")
        .unwrap()
        .integration(home, &|_| None)
        .unwrap()
}

#[test]
fn opencode_s_plugin_is_a_file_and_an_entry_in_its_plugin_list() {
    let plan = opencode_plan(Path::new("/Users/you"));
    assert_eq!(plan.file, Path::new("/Users/you/.config/opencode/cli.json"));
    let moved = adapter("opencode")
        .unwrap()
        .integration(Path::new("/Users/you"), &|k| {
            (k == "XDG_CONFIG_HOME").then(|| "/work/config".to_string())
        })
        .unwrap();
    assert_eq!(moved.file, Path::new("/work/config/opencode/cli.json"));
}

#[test]
fn opencode_s_plugin_goes_beside_the_person_s_and_comes_out_whole() {
    let home = tempfile::tempdir().unwrap();
    let plan = opencode_plan(home.path());
    let folder = plan.file.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    // the list as herdr leaves it
    let original = "{\n  \"plugins\": [\"./herdr-opencode\"]\n}\n";
    std::fs::write(&plan.file, original).unwrap();
    assert_eq!(file_state(&plan), HookState::Absent);
    assert_eq!(apply(&plan, true).unwrap(), HookState::Current);
    let list: Value = serde_json::from_str(&std::fs::read_to_string(&plan.file).unwrap()).unwrap();
    assert_eq!(
        list["plugins"],
        json!(["./herdr-opencode", "./pinrail-opencode"])
    );
    let plugin = folder.join("pinrail-opencode").join("tui.js");
    assert!(
        std::fs::read_to_string(&plugin)
            .unwrap()
            .contains("\"hook\", \"opencode\"")
    );
    // installed again, nothing changes
    assert_eq!(apply(&plan, true).unwrap(), HookState::Current);
    let again: Value = serde_json::from_str(&std::fs::read_to_string(&plan.file).unwrap()).unwrap();
    assert_eq!(again, list);
    // the plugin file gone or of another version: on again repairs it
    std::fs::remove_file(&plugin).unwrap();
    assert_eq!(file_state(&plan), HookState::Outdated);
    assert_eq!(apply(&plan, true).unwrap(), HookState::Current);
    // off takes out the entry and the file, and leaves the person's
    assert_eq!(apply(&plan, false).unwrap(), HookState::Absent);
    let after: Value = serde_json::from_str(&std::fs::read_to_string(&plan.file).unwrap()).unwrap();
    assert_eq!(after["plugins"], json!(["./herdr-opencode"]));
    assert!(!plugin.exists());
    // a list Pinrail cannot edit safely is left alone
    std::fs::write(&plan.file, "{\"plugins\": \"./mine\"}").unwrap();
    assert_eq!(file_state(&plan), HookState::Unreadable);
    assert!(apply(&plan, true).is_err());
}

fn codex_plan() -> Integration {
    adapter("codex")
        .unwrap()
        .integration(Path::new("/Users/you"), &|_| None)
        .unwrap()
}

#[test]
fn codex_s_hooks_go_beside_the_person_s_in_its_hooks_file() {
    let plan = codex_plan();
    assert_eq!(plan.file, Path::new("/Users/you/.codex/hooks.json"));
    let moved = adapter("codex")
        .unwrap()
        .integration(Path::new("/Users/you"), &|k| {
            (k == "CODEX_HOME").then(|| "/work/codex".to_string())
        })
        .unwrap();
    assert_eq!(moved.file, Path::new("/work/codex/hooks.json"));
    // the file as herdr leaves it
    let original = json!({
        "hooks": {"SessionStart": [{"hooks": [{
            "command": "bash '/Users/you/.codex/herdr-agent-state.sh' session",
            "timeout": 10,
            "type": "command"
        }]}]}
    });
    let mut hooks = original.clone();
    install(&mut hooks, &plan);
    assert_eq!(state(Some(&hooks), &plan), HookState::Current);
    assert_eq!(hooks["hooks"]["SessionStart"].as_array().unwrap().len(), 2);
    assert_eq!(
        hooks["hooks"]["UserPromptSubmit"][0]["hooks"][0],
        json!({
            "type": "command",
            "command": format!("{COMMAND_PREFIX} codex prompt"),
            "timeout": 2,
            "async": true
        })
    );
    // Codex cannot run the end of a session in the background
    assert_eq!(
        hooks["hooks"]["SessionEnd"][0]["hooks"][0].get("async"),
        None
    );
    remove(&mut hooks, &plan);
    assert_eq!(hooks, original);
}

#[test]
fn a_codex_hook_waited_for_or_edited_by_hand_is_told_apart() {
    let plan = codex_plan();
    let mut hooks = json!({});
    install(&mut hooks, &plan);
    // as a version that did not run it in the background wrote it
    let mut waited = hooks.clone();
    waited["hooks"]["Stop"][0]["hooks"][0]
        .as_object_mut()
        .unwrap()
        .remove("async");
    assert_eq!(state(Some(&waited), &plan), HookState::Outdated);
    let mut edited = hooks.clone();
    edited["hooks"]["Stop"][0]["hooks"][0]["async"] = json!(false);
    assert_eq!(state(Some(&edited), &plan), HookState::Modified);
}

#[test]
fn every_codex_hook_installed_is_one_pinrail_hook_reads() {
    let known = [
        "session-start",
        "prompt",
        "tool",
        "permission",
        "interrupt",
        "stop",
        "session-end",
    ];
    for entry in codex_plan().entries {
        let hook = entry.command.rsplit(' ').next().unwrap();
        assert!(known.contains(&hook), "{}", entry.command);
        assert!(entry.command.contains(" hook codex "));
    }
}

#[test]
fn codex_s_rule_for_the_pinrail_command_goes_in_its_rules_folder() {
    let codex = adapter("codex").unwrap();
    let files = codex.skill_files(Path::new("/Users/you"), &|_| None);
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0].path,
        Path::new("/Users/you/.codex/rules/pinrail.rules")
    );
    assert!(files[0].text.starts_with(files[0].marker));
    let moved = codex.skill_files(Path::new("/Users/you"), &|k| {
        (k == "CODEX_HOME").then(|| "/work/codex".to_string())
    });
    assert_eq!(moved[0].path, Path::new("/work/codex/rules/pinrail.rules"));
}
