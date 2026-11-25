//! Claude Code: what each of its hooks reports about its session.

use serde_json::Value;

use std::path::Path;

use crate::agent::{
    Agent, Env, Hint, HintStatus, PASTE_AND_ENTER, StartPlan, StartRequest, SubmitPlan, push_task,
    resumable_id, spec,
};
use crate::hooks::{HookEvent, HookReport, TurnEnd, WaitKind, common, start_source, summary, text};
use crate::integration::{HookEntry, Integration, Kind, Layout};
use crate::registry::AgentSpec;

pub struct ClaudeCode;

/// How Claude Code begins the prompts it gives itself rather than the
/// person: a subagent's result handed back to the main session, and the
/// notice that a background command finished. Each fires
/// `UserPromptSubmit` without an `agent_id`, but the turn goes on.
const OWN_PROMPTS: [&str; 2] = ["<agent-message ", "<task-notification>"];

impl Agent for ClaudeCode {
    fn spec(&self) -> &'static AgentSpec {
        spec("claude-code")
    }

    fn parse_hook(&self, event: &str, input: &Value, env: Env) -> Option<HookReport> {
        // a subagent's hook does not speak for the session; `agent_type`
        // alone is a main session started with --agent
        if input.get("agent_id").is_some() {
            return None;
        }
        let session = text(input, "session_id")?;
        // only Claude Code's own hooks: it runs them with its session's id in
        // the environment. Cursor and Grok run its hooks too, for sessions of
        // their own, and do not set it, or inherit another session's.
        if env("CLAUDE_CODE_SESSION_ID").as_deref() != Some(session.as_str()) {
            return None;
        }
        let mut detail = common(input);
        let event = match event {
            "session-start" => HookEvent::Started {
                source: start_source(input)?,
            },
            "prompt" => {
                let prompt = text(input, "prompt");
                if prompt
                    .as_deref()
                    .is_some_and(|p| OWN_PROMPTS.iter().any(|own| p.starts_with(own)))
                {
                    HookEvent::Working
                } else {
                    detail.prompt = prompt;
                    HookEvent::Prompt
                }
            }
            "permission" => {
                detail.tool = text(input, "tool_name");
                detail.summary = summary(input.get("tool_input"));
                HookEvent::NeedsInput {
                    kind: WaitKind::Permission,
                }
            }
            // the call was refused, and the turn goes on
            "permission-denied" | "tool-done" => HookEvent::Working,
            "notification" => HookEvent::NeedsInput {
                kind: match input.get("notification_type")?.as_str()? {
                    "permission_prompt" => WaitKind::Permission,
                    "elicitation_dialog" | "agent_needs_input" => WaitKind::Question,
                    _ => return None,
                },
            },
            "stop" => {
                detail.reply = text(input, "last_assistant_message");
                HookEvent::TurnEnded { end: TurnEnd::Done }
            }
            "stop-failure" => {
                // the error as Claude Code shows it, then its details, then
                // only its type, such as `server_error`
                detail.error = text(input, "last_assistant_message")
                    .map(|shown| match shown.strip_prefix("API Error: ") {
                        Some(rest) => rest.to_string(),
                        None => shown,
                    })
                    .or_else(|| text(input, "error_details"))
                    .or_else(|| text(input, "error"));
                HookEvent::TurnEnded {
                    end: TurnEnd::Failed,
                }
            }
            "session-end" => {
                detail.reason = text(input, "reason");
                HookEvent::Ended
            }
            _ => return None,
        };
        Some(HookReport {
            session,
            event,
            detail,
        })
    }

    /// Claude Code's own `settings.json`, with a hook for each event that
    /// says what the session does: its start, each prompt, a permission it
    /// asks or that is refused, a notification that it waits on the person,
    /// each tool call that ends, and the end of each turn and of the session.
    fn integration(&self, home: &Path, env: Env) -> Option<Integration> {
        let entry = |event, matcher, hook| HookEntry::new("claude-code", event, matcher, hook);
        Some(Integration {
            kind: Kind::Hooks,
            file: config_folder(home, env).join("settings.json"),
            layout: Layout::Settings,
            entries: vec![
                entry("SessionStart", None, "session-start"),
                entry("UserPromptSubmit", None, "prompt"),
                entry("PermissionRequest", Some("*"), "permission"),
                entry("PermissionDenied", Some("*"), "permission-denied"),
                entry(
                    "Notification",
                    Some("permission_prompt|elicitation_dialog|agent_needs_input"),
                    "notification",
                ),
                entry("PostToolUse", Some("*"), "tool-done"),
                entry("PostToolUseFailure", Some("*"), "tool-done"),
                entry("Stop", None, "stop"),
                entry("StopFailure", None, "stop-failure"),
                entry("SessionEnd", None, "session-end"),
            ],
        })
    }

    /// Claude Code keeps a file per running process, `sessions/<pid>.json`
    /// in its configuration folder, with the session it holds and whether
    /// it is busy, idle or waiting. It changes within seconds of an
    /// interrupt, which runs no hook. The file is undocumented, so anything
    /// unexpected gives no hint.
    fn presence_hint(&self, home: &Path, env: Env, pid: u32, session: &str) -> Option<Hint> {
        let folder = config_folder(home, env);
        let text =
            std::fs::read_to_string(folder.join("sessions").join(format!("{pid}.json"))).ok()?;
        status_hint(&serde_json::from_str(&text).ok()?, session)
    }

    /// `claude --session-id <uuid> [extra args] [task]`: Pinrail chooses the
    /// id, so the session exists from the start.
    fn start(&self, request: &StartRequest) -> Option<StartPlan> {
        let mut argv = vec![
            "claude".to_string(),
            "--session-id".to_string(),
            request.session_id.to_string(),
        ];
        argv.extend(request.extra_args.iter().cloned());
        push_task(&mut argv, request.task);
        Some(StartPlan {
            argv,
            session: Some(request.session_id.to_string()),
            type_task: None,
        })
    }

    /// `claude --resume <id>`.
    fn resume(&self, session: &str) -> Option<Vec<String>> {
        resumable_id(session).then(|| {
            vec![
                "claude".to_string(),
                "--resume".to_string(),
                session.to_string(),
            ]
        })
    }

    /// Two Claude Code processes can hold one session, each adding to it.
    fn resumes_while_running(&self) -> bool {
        true
    }

    /// A bracketed paste and Enter, confirmed by `UserPromptSubmit`.
    fn submit(&self) -> Option<SubmitPlan> {
        Some(PASTE_AND_ENTER)
    }

    /// A rule that allows reading the folder, given with `--settings`.
    /// `--add-dir` would ask the person to trust the folder in every
    /// session, and would let Claude Code edit and run files there too.
    fn read_folder_args(&self, folder: &str) -> Vec<String> {
        // an absolute path in a rule starts with `//`
        let rule = format!("Read(/{}/**)", folder.trim_end_matches('/'));
        let settings = serde_json::json!({ "permissions": { "allow": [rule] } });
        vec!["--settings".to_string(), settings.to_string()]
    }

    /// The native install links `claude` to
    /// `~/.local/share/claude/versions/<version>`, and the system names the
    /// process after that file: `2.1.331`.
    fn is_own_process(&self, process: &crate::process::Process) -> bool {
        let name = process.name.as_str();
        name == "claude"
            || (name.contains('.')
                && name.starts_with(|c: char| c.is_ascii_digit())
                && name
                    .split('.')
                    .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())))
    }

    fn deny_flags(&self) -> &'static [&'static str] {
        &[
            "--dangerously-skip-permissions",
            "--allow-dangerously-skip-permissions",
        ]
    }
}

/// Claude Code's configuration folder: `CLAUDE_CONFIG_DIR`, or `~/.claude`.
fn config_folder(home: &Path, env: Env) -> std::path::PathBuf {
    env("CLAUDE_CONFIG_DIR")
        .filter(|d| !d.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"))
}

/// The hint in a status file, if it is about `session`.
fn status_hint(file: &Value, session: &str) -> Option<Hint> {
    if file.get("sessionId")?.as_str()? != session {
        return None;
    }
    let status = match file.get("status")?.as_str()? {
        "busy" => HintStatus::Busy,
        "idle" => HintStatus::Idle,
        "waiting" => HintStatus::Waiting,
        _ => return None,
    };
    Some(Hint {
        status,
        since_ms: file.get("statusUpdatedAt")?.as_i64()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::testing::{Case, connects_and_removes, no_shell};

    #[test]
    fn it_is_connected_and_removed_with_its_hooks() {
        let case = Case {
            config: ".claude",
            skills: ".claude/skills",
            reporting: Some(".claude/settings.json"),
            theirs: Some(
                r#"{"model": "opus", "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "my-own-stop"}]}]}}"#,
            ),
        };
        connects_and_removes(&ClaudeCode, &case, &no_shell);
    }
}
