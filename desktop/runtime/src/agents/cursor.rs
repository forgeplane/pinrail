//! Cursor's CLI (`cursor-agent`): what each of its hooks reports about its
//! conversation. Its hooks carry snake_case fields with the conversation's
//! id and a `generation_id` for each prompt; the reply comes in its own
//! event beside the turn's stop, in either order. It has no hook for a
//! permission it asks. Checked live against cursor-agent 2026.11.19.

use std::path::Path;

use serde_json::Value;

use crate::agent::{Agent, Env, spec};
use crate::hooks::{Detail, HookEvent, HookReport, StartSource, TurnEnd, text};
use crate::integration::{HookEntry, Integration, Kind, Layout};
use crate::process::Process;
use crate::registry::AgentSpec;

pub struct Cursor;

impl Agent for Cursor {
    fn spec(&self) -> &'static AgentSpec {
        spec("cursor")
    }

    fn parse_hook(&self, event: &str, input: &Value, _env: Env) -> Option<HookReport> {
        // a background agent's conversation is not the one in the terminal
        if input.get("is_background_agent").and_then(Value::as_bool) == Some(true) {
            return None;
        }
        let session = text(input, "conversation_id")?;
        let mut detail = Detail {
            transcript_path: text(input, "transcript_path"),
            turn_id: text(input, "generation_id"),
            cwd: input
                .pointer("/workspace_roots/0")
                .and_then(Value::as_str)
                .map(str::to_string),
            ..Detail::default()
        };
        let event = match event {
            "session-start" => HookEvent::Started {
                source: StartSource::Startup,
            },
            "prompt" => {
                detail.prompt = text(input, "prompt");
                HookEvent::Prompt
            }
            "tool" => HookEvent::Working,
            // the reply ends the turn: the stop beside it, in either order,
            // would end it again without one
            "response" => {
                detail.reply = text(input, "text");
                HookEvent::TurnEnded { end: TurnEnd::Done }
            }
            // Ctrl+C sends both an aborted and an error stop, which the core
            // ends as interrupted
            "stop" => HookEvent::TurnEnded {
                end: match text(input, "status").as_deref() {
                    Some("aborted") => TurnEnd::Interrupted,
                    Some("error") => TurnEnd::Failed,
                    _ => return None,
                },
            },
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

    /// Cursor's user hooks file, `~/.cursor/hooks.json`, beside the person's
    /// own entries: for the start of a conversation, each prompt, each tool
    /// call, the reply, the end of each turn and of the conversation.
    fn integration(&self, home: &Path, _env: Env) -> Option<Integration> {
        let entry = |event, hook| HookEntry::new("cursor", event, None, hook);
        Some(Integration {
            kind: Kind::Hooks,
            file: home.join(".cursor").join("hooks.json"),
            layout: Layout::Flat,
            entries: vec![
                entry("sessionStart", "session-start"),
                entry("beforeSubmitPrompt", "prompt"),
                entry("preToolUse", "tool"),
                entry("postToolUse", "tool"),
                entry("afterAgentResponse", "response"),
                entry("stop", "stop"),
                entry("sessionEnd", "session-end"),
            ],
        })
    }

    /// Cursor's CLI runs as `node`, with the command it was invoked as,
    /// `cursor-agent` or `agent`, as its first argument; its worker
    /// processes are `node` too.
    fn is_own_process(&self, process: &Process) -> bool {
        if process.name == "cursor-agent" {
            return true;
        }
        process.name == "node"
            && crate::process::arguments(process.pid).is_some_and(|args| is_cursor_command(&args))
    }
}

/// Whether a `node` process's arguments are Cursor's CLI: invoked as
/// `cursor-agent` or `agent`, and not one of its workers.
pub fn is_cursor_command(args: &[String]) -> bool {
    let invoked = args
        .first()
        .and_then(|first| Path::new(first).file_name())
        .and_then(|name| name.to_str());
    matches!(invoked, Some("cursor-agent" | "agent"))
        && !args.iter().any(|arg| arg == "worker-server")
}
