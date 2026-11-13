//! Codex: its hooks, as `runtime-spec.md` section 5.4 lists them. The
//! fields are those of `codex-rs/hooks/schema/generated/`.

use serde_json::Value;

use crate::agent::{Agent, Env, spec};
use crate::hooks::{HookEvent, HookReport, TurnEnd, WaitKind, common, start_source, summary, text};
use crate::registry::AgentSpec;

pub struct Codex;

impl Agent for Codex {
    fn spec(&self) -> &'static AgentSpec {
        spec("codex")
    }

    fn parse_hook(&self, event: &str, input: &Value, _env: Env) -> Option<HookReport> {
        // a subagent's hook carries the root session's id, and does not
        // speak for it
        if input.get("agent_id").is_some() {
            return None;
        }
        let session = text(input, "session_id")?;
        let mut detail = common(input);
        let event = match event {
            "session-start" => HookEvent::Started {
                source: start_source(input)?,
            },
            "prompt" => {
                detail.prompt = text(input, "prompt");
                HookEvent::Prompt
            }
            "tool" => HookEvent::Working,
            "permission" => {
                detail.tool = text(input, "tool_name");
                detail.summary = summary(input.get("tool_input"));
                HookEvent::NeedsInput {
                    kind: WaitKind::Permission,
                }
            }
            "interrupt" => HookEvent::TurnEnded {
                end: TurnEnd::Interrupted,
            },
            "stop" => {
                detail.reply = text(input, "last_assistant_message");
                HookEvent::TurnEnded { end: TurnEnd::Done }
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
