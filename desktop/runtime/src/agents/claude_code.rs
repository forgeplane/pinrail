//! Claude Code: its hooks, as `runtime-spec.md` section 5.3 lists them.

use serde_json::Value;

use crate::agent::{Agent, Env, spec};
use crate::hooks::{HookEvent, HookReport, TurnEnd, WaitKind, common, start_source, summary, text};
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
}
