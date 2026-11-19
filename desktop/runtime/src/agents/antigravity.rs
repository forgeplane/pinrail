//! Antigravity CLI (`agy`): what each of its hooks reports about its
//! conversation. Its hooks carry camelCase fields, start a conversation at
//! its first prompt rather than at launch, and carry no reply, which its
//! transcript holds. Checked live against agy 1.5.12.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::Value;

use crate::agent::{Agent, Env, spec};
use crate::hooks::{Detail, HookEvent, HookReport, StartSource, TurnEnd, text};
use crate::integration::{HookEntry, HookPlan, Layout};
use crate::registry::AgentSpec;

pub struct Antigravity;

/// The most of a transcript's end read for the reply: agy writes a line
/// per step, and the reply is among the last.
const TRANSCRIPT_TAIL: u64 = 1024 * 1024;

impl Agent for Antigravity {
    fn spec(&self) -> &'static AgentSpec {
        spec("antigravity")
    }

    fn parse_hook(&self, event: &str, input: &Value, env: Env) -> Option<HookReport> {
        let session = text(input, "conversationId")?;
        // only the conversation's own hooks: agy runs them with its id in the
        // environment, and a subagent's conversation has another
        if env("ANTIGRAVITY_CONVERSATION_ID").as_deref() != Some(session.as_str()) {
            return None;
        }
        let mut detail = Detail {
            transcript_path: text(input, "transcriptPath"),
            cwd: text(input, "cwd"),
            ..Detail::default()
        };
        let event = match event {
            // at the first prompt of a new conversation; a resumed one
            // fires none
            "session-start" => HookEvent::Started {
                source: StartSource::Startup,
            },
            // each model call; the first of a turn carries its prompt
            "invocation" if input.get("invocationNum")?.as_i64()? == 0 => {
                detail.prompt = text(input, "lastUserInput");
                HookEvent::Prompt
            }
            "invocation" | "tool-done" => HookEvent::Working,
            "stop" => {
                let reason = text(input, "terminationReason").unwrap_or_default();
                let end = match reason.to_ascii_uppercase().as_str() {
                    "USER_CANCELED" => TurnEnd::Interrupted,
                    "ERROR"
                    | "MAX_INVOCATIONS"
                    | "MAX_FORCED_INVOCATIONS"
                    | "MAX_TOKEN_BUDGET_EXCEEDED"
                    | "MAX_STEPS_EXCEEDED" => TurnEnd::Failed,
                    _ => TurnEnd::Done,
                };
                match end {
                    TurnEnd::Done => {
                        detail.reply = detail
                            .transcript_path
                            .as_deref()
                            .and_then(|path| reply(Path::new(path)));
                    }
                    TurnEnd::Failed => {
                        detail.error =
                            text(input, "error").or(Some(reason).filter(|r| !r.is_empty()));
                    }
                    TurnEnd::Interrupted => {}
                }
                HookEvent::TurnEnded { end }
            }
            _ => return None,
        };
        Some(HookReport {
            session,
            event,
            detail,
        })
    }

    /// A hook named `pinrail` in agy's global hooks file,
    /// `~/.gemini/config/hooks.json`, beside the person's own: for the
    /// start of a conversation, each model call, each tool call that ends,
    /// and the end of each turn. agy has no hook for a permission or a
    /// question: its terminal's OSC 7501 records say when it waits.
    fn hook_plan(&self, home: &Path, _env: Env) -> Option<HookPlan> {
        let entry = |event, matcher, hook| HookEntry::new("antigravity", event, matcher, hook);
        Some(HookPlan {
            file: home.join(".gemini").join("config").join("hooks.json"),
            layout: Layout::Named("pinrail"),
            entries: vec![
                entry("SessionStart", None, "session-start"),
                entry("PreInvocation", None, "invocation"),
                entry("PostToolUse", Some("*"), "tool-done"),
                entry("Stop", None, "stop"),
            ],
        })
    }
}

/// The agent's last message, from the transcript agy keeps: the content of
/// the last `PLANNER_RESPONSE` step that has any. The file is
/// undocumented, so anything unexpected gives no reply.
fn reply(transcript: &Path) -> Option<String> {
    let mut file = std::fs::File::open(transcript).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(TRANSCRIPT_TAIL)))
        .ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    String::from_utf8_lossy(&bytes)
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|step| step.get("type").and_then(Value::as_str) == Some("PLANNER_RESPONSE"))
        .find_map(|step| text(&step, "content"))
}
