//! Claude Code: what each of its hooks reports about its session.

use serde_json::Value;

use std::path::Path;

use crate::agent::{Agent, Env, Hint, HintStatus, spec};
use crate::hooks::{HookEvent, HookReport, TurnEnd, WaitKind, common, start_source, summary, text};
use crate::integration::{HookEntry, HookPlan};
use crate::registry::AgentSpec;

pub struct ClaudeCode;

/// How Claude Code begins the prompt that hands a subagent's result back to
/// the main session. It fires `UserPromptSubmit` without an `agent_id`, but
/// the turn goes on.
const HANDBACK: &str = "<agent-message ";

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
                if prompt.as_deref().is_some_and(|p| p.starts_with(HANDBACK)) {
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
                detail.error = text(input, "error").or_else(|| text(input, "error_details"));
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
    fn hook_plan(&self, home: &Path, env: Env) -> Option<HookPlan> {
        let entry = |event, matcher, hook| HookEntry::new("claude-code", event, matcher, hook);
        Some(HookPlan {
            file: config_folder(home, env).join("settings.json"),
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
