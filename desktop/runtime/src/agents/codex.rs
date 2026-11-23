//! Codex: what each of its hooks reports about its session. The
//! fields are those of `codex-rs/hooks/schema/generated/`.

use std::path::Path;

use serde_json::Value;

use crate::agent::{
    Agent, Env, PASTE_AND_ENTER, StartPlan, StartRequest, SubmitPlan, push_task, resumable_id, spec,
};
use crate::hooks::{HookEvent, HookReport, TurnEnd, WaitKind, common, start_source, summary, text};
use crate::integration::{HookEntry, Integration, Kind, Layout};
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

    /// Pinrail's groups in Codex's hooks file, `~/.codex/hooks.json` (or
    /// `$CODEX_HOME/hooks.json`), beside the person's: for the start of a
    /// session, each prompt, each finished tool call, a permission, an
    /// interrupt, and the end of each turn and of the session. Codex runs
    /// them without waiting, except at the end of the session, which it
    /// cannot run in the background. Codex runs no hook until the person
    /// has trusted it.
    fn integration(&self, home: &Path, env: Env) -> Option<Integration> {
        let entry = |event, hook| HookEntry::new("codex", event, None, hook);
        let folder = env("CODEX_HOME")
            .filter(|h| !h.trim().is_empty())
            .map(Into::into)
            .unwrap_or_else(|| home.join(".codex"));
        Some(Integration {
            kind: Kind::Hooks,
            file: folder.join("hooks.json"),
            layout: Layout::Settings,
            entries: vec![
                entry("SessionStart", "session-start").in_background(),
                entry("UserPromptSubmit", "prompt").in_background(),
                entry("PostToolUse", "tool").in_background(),
                entry("PermissionRequest", "permission").in_background(),
                entry("Interrupt", "interrupt").in_background(),
                entry("Stop", "stop").in_background(),
                entry("SessionEnd", "session-end"),
            ],
        })
    }

    /// `codex --no-daemon [extra args] [task]`. Without the daemon, the
    /// session, its hooks and its tool commands run in the terminal's own
    /// process, so they see the run's environment. Codex chooses the
    /// session's id, which the run learns from its first `SessionStart`.
    fn start(&self, request: &StartRequest) -> Option<StartPlan> {
        let mut argv = vec!["codex".to_string(), "--no-daemon".to_string()];
        argv.extend(request.extra_args.iter().cloned());
        push_task(&mut argv, request.task);
        Some(StartPlan {
            argv,
            session: None,
        })
    }

    /// `codex --no-daemon resume <id>`.
    fn resume(&self, session: &str) -> Option<Vec<String>> {
        resumable_id(session).then(|| {
            ["codex", "--no-daemon", "resume", session]
                .map(str::to_string)
                .to_vec()
        })
    }

    /// A bracketed paste and Enter, confirmed by `UserPromptSubmit`.
    fn submit(&self) -> Option<SubmitPlan> {
        Some(PASTE_AND_ENTER)
    }

    fn deny_flags(&self) -> &'static [&'static str] {
        &[
            "--dangerously-bypass-approvals-and-sandbox",
            "--dangerously-bypass-hook-trust",
        ]
    }
}
