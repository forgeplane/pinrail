//! Cursor's CLI (`cursor-agent`): what each of its hooks reports about its
//! conversation. Its hooks carry snake_case fields with the conversation's
//! id and a `generation_id` for each prompt; the reply comes in its own
//! event beside the turn's stop, in either order. It has no hook for a
//! permission it asks. Checked live against cursor-agent 2026.11.19.

use std::path::Path;

use serde_json::Value;

use crate::agent::{
    Agent, Env, PASTE_AND_ENTER, StartPlan, StartRequest, SubmitPlan, resumable_id, spec,
};
use crate::hooks::{Detail, HookEvent, HookReport, StartSource, TurnEnd, WaitKind, text};
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
    /// `cursor-agent --resume <uuid> [extra args] [agent -- <task>]`: a new
    /// id to `--resume` creates the chat with it, so Pinrail chooses it.
    /// The task goes after Cursor's `agent` command: after `--` alone, a
    /// task such as `help` would still run that command.
    fn start(&self, request: &StartRequest) -> Option<StartPlan> {
        let mut argv = vec![
            "cursor-agent".to_string(),
            "--resume".to_string(),
            request.session_id.to_string(),
        ];
        argv.extend(request.extra_args.iter().cloned());
        if let Some(task) = request.task.filter(|t| !t.trim().is_empty()) {
            argv.extend(["agent".to_string(), "--".to_string(), task.to_string()]);
        }
        Some(StartPlan {
            argv,
            session: Some(request.session_id.to_string()),
            type_task: None,
        })
    }

    /// `cursor-agent --resume <id>`.
    fn resume(&self, session: &str) -> Option<Vec<String>> {
        resumable_id(session).then(|| {
            ["cursor-agent", "--resume", session]
                .map(str::to_string)
                .to_vec()
        })
    }

    /// Ctrl+U, then a bracketed paste and Enter: after an interrupt, Cursor
    /// puts the interrupted prompt back in its input, which Ctrl+U clears.
    /// The prompt's hook confirms it.
    fn submit(&self) -> Option<SubmitPlan> {
        Some(SubmitPlan {
            clear: "\x15",
            ..PASTE_AND_ENTER
        })
    }

    fn skip_checks_flags(&self) -> &'static [&'static str] {
        &["--force", "-f", "--yolo"]
    }

    /// An example of the person's arguments, from its `--help`.
    fn args_example(&self) -> Option<&'static str> {
        Some("--model sonnet-4-thinking")
    }

    /// The flags Pinrail sets to name, resume or prompt the session.
    fn own_flags(&self) -> &'static [&'static str] {
        &["--resume"]
    }

    /// Cursor sends its notifications only to the terminals it recognises
    /// by `TERM_PROGRAM`, which Pinrail is not. Pinrail's terminal is built
    /// on Ghostty's terminal library, so what Cursor expects of Ghostty
    /// holds, its OSC 777 notifications among it.
    fn run_env(&self) -> &'static [(&'static str, &'static str)] {
        &[("TERM_PROGRAM", "ghostty")]
    }

    /// Cursor has no hook for a permission it asks: it says so in a
    /// notification, "Approve command: <command>", while its terminal is
    /// not focused. Its other notification, that it waits for the next
    /// prompt, the end of the turn's hooks say already.
    fn notification(&self, title: Option<&str>, body: &str) -> Option<(WaitKind, Option<String>)> {
        if title != Some("Cursor") {
            return None;
        }
        let command = body.strip_prefix("Approve command: ")?;
        Some((
            WaitKind::Permission,
            Some(command.chars().take(200).collect()),
        ))
    }

    /// /clear starts a new conversation with no session start: its first
    /// prompt is the first sign.
    fn prompt_may_start_session(&self) -> bool {
        true
    }

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
    // on Windows, node on the script in Cursor's install folder
    (matches!(invoked, Some("cursor-agent" | "agent"))
        || super::runs_package_script(args, "cursor-agent"))
        && !args.iter().any(|arg| arg == "worker-server")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_is_known_by_its_package_s_script_on_windows() {
        let args = |list: &[&str]| list.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        assert!(is_cursor_command(&args(&[
            r"C:\Program Files\nodejs\node.exe",
            r"C:\Users\me\AppData\Local\cursor-agent\versions\2025.10.01\index.js",
        ])));
        assert!(!is_cursor_command(&args(&[
            r"C:\nodejs\node.exe",
            r"C:\Users\me\AppData\Local\cursor-agent\versions\2025.10.01\index.js",
            "worker-server",
        ])));
    }

    #[test]
    fn it_knows_the_flags_that_turn_its_checks_off() {
        crate::agent::testing::recognizes(
            &Cursor,
            &[
                (&["--force"], "--force"),
                (&["-f"], "-f"),
                (&["--yolo"], "--yolo"),
            ],
            &[&["--model", "gpt-5"]],
        );
    }
    use crate::connection::testing::{Case, connects_and_removes, no_shell};

    #[test]
    fn it_is_connected_and_removed_with_its_hooks() {
        let case = Case {
            config: ".cursor",
            skills: ".cursor/skills",
            reporting: Some(".cursor/hooks.json"),
            theirs: Some(
                r#"{"version": 1, "hooks": {"sessionStart": [{"command": "my-own-start"}]}}"#,
            ),
        };
        connects_and_removes(&Cursor, &case, &no_shell);
    }
}
