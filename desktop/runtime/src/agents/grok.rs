//! Grok CLI: what each of its hooks reports about its session. Its hooks
//! take Claude Code's format and carry each field twice, in camelCase and
//! in snake_case; the end of a turn carries its reply. Checked live against
//! Grok 1.0.89.

use std::path::Path;

use serde_json::Value;

use crate::agent::{
    Agent, Env, PASTE_AND_ENTER, StartPlan, StartRequest, SubmitPlan, push_task, resumable_id, spec,
};
use crate::hooks::{Detail, HookEvent, HookReport, StartSource, TurnEnd, WaitKind, text};
use crate::integration::{HookEntry, HookPlan, Layout};
use crate::registry::AgentSpec;

pub struct Grok;

impl Agent for Grok {
    fn spec(&self) -> &'static AgentSpec {
        spec("grok")
    }

    fn parse_hook(&self, event: &str, input: &Value, env: Env) -> Option<HookReport> {
        // a subagent's hook does not speak for the session
        if input.get("subagentType").is_some() {
            return None;
        }
        let session = text(input, "sessionId")?;
        // only the session's own hooks: Grok runs them with its id in the
        // environment, and runs Claude Code's hooks too
        if env("GROK_SESSION_ID").as_deref() != Some(session.as_str()) {
            return None;
        }
        let mut detail = Detail {
            transcript_path: text(input, "transcriptPath"),
            turn_id: text(input, "promptId"),
            cwd: text(input, "cwd"),
            ..Detail::default()
        };
        let event = match event {
            // at launch, `new`; on `-r`, `load`. /clear starts a new
            // session in the same process with `new` too.
            "session-start" => HookEvent::Started {
                source: match input.get("source")?.as_str()? {
                    "new" => StartSource::Startup,
                    "load" => StartSource::Resume,
                    _ => return None,
                },
            },
            "prompt" => {
                detail.prompt = text(input, "prompt");
                HookEvent::Prompt
            }
            // the question tool waits for the person's answer, and is the
            // only report that says the question
            "tool" if text(input, "toolName").as_deref() == Some("ask_user_question") => {
                detail.summary = input
                    .pointer("/toolInput/questions/0/question")
                    .and_then(Value::as_str)
                    .map(|q| q.chars().take(200).collect());
                HookEvent::NeedsInput {
                    kind: WaitKind::Question,
                }
            }
            // a refused call ends the turn, which its own cancel reports
            "tool" | "permission-denied" => HookEvent::Working,
            // the tool a permission is for came with the tool call before
            // it, which cannot know that a permission follows
            "notification" => HookEvent::NeedsInput {
                kind: match text(input, "notificationType")?.as_str() {
                    "permission_prompt" => WaitKind::Permission,
                    "elicitation_dialog" => WaitKind::Question,
                    _ => return None,
                },
            },
            // Grok also fires Stop as it shuts down, which ends no turn
            "stop" if text(input, "reason").as_deref() == Some("end_turn") => {
                detail.reply = text(input, "lastAssistantMessage");
                HookEvent::TurnEnded { end: TurnEnd::Done }
            }
            "stop-failure" => {
                detail.error = text(input, "error").or_else(|| text(input, "errorDetails"));
                HookEvent::TurnEnded {
                    end: TurnEnd::Failed,
                }
            }
            // Ctrl+C, and a permission the person declined or dismissed
            "stop-cancelled" => HookEvent::TurnEnded {
                end: match text(input, "reason").as_deref() {
                    Some("user_interrupt" | "permission_rejected" | "permission_cancelled") => {
                        TurnEnd::Interrupted
                    }
                    _ => {
                        detail.error = text(input, "reasonDetails");
                        TurnEnd::Failed
                    }
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

    /// A file of Pinrail's own in Grok's hooks folder, `~/.grok/hooks/` (or
    /// `$GROK_HOME/hooks/`), which Grok loads beside the person's files:
    /// for the start of a session, each prompt, each tool call, a
    /// notification that it waits on the person, a refused permission, and
    /// the end of each turn and of the session. Grok runs no hook in a
    /// folder the person has not trusted.
    fn hook_plan(&self, home: &Path, env: Env) -> Option<HookPlan> {
        let entry = |event, hook| HookEntry::new("grok", event, None, hook);
        let folder = env("GROK_HOME")
            .filter(|h| !h.trim().is_empty())
            .map(Into::into)
            .unwrap_or_else(|| home.join(".grok"));
        Some(HookPlan {
            file: folder.join("hooks").join("pinrail.json"),
            layout: Layout::Settings,
            entries: vec![
                entry("SessionStart", "session-start"),
                entry("UserPromptSubmit", "prompt"),
                entry("PreToolUse", "tool"),
                entry("PostToolUse", "tool"),
                entry("PostToolUseFailure", "tool"),
                entry("Notification", "notification"),
                entry("PermissionDenied", "permission-denied"),
                entry("Stop", "stop"),
                entry("StopFailure", "stop-failure"),
                entry("StopCancelled", "stop-cancelled"),
                entry("SessionEnd", "session-end"),
            ],
        })
    }

    /// `grok -s <uuid> [extra args] -- [task]`: Pinrail chooses the id, so
    /// the session exists from the start. A task after `--` is the prompt
    /// even when it is the name of one of Grok's commands.
    fn start(&self, request: &StartRequest) -> Option<StartPlan> {
        let mut argv = vec![
            "grok".to_string(),
            "-s".to_string(),
            request.session_id.to_string(),
        ];
        argv.extend(request.extra_args.iter().cloned());
        push_task(&mut argv, request.task);
        Some(StartPlan {
            argv,
            session: Some(request.session_id.to_string()),
        })
    }

    /// `grok -r <id>`. An id Grok does not know exits at once.
    fn resume(&self, session: &str) -> Option<Vec<String>> {
        resumable_id(session)
            .then(|| vec!["grok".to_string(), "-r".to_string(), session.to_string()])
    }

    /// A bracketed paste and Enter, which Grok takes as one prompt with its
    /// line breaks, confirmed by `UserPromptSubmit`. A message sent while it
    /// works waits in Grok's queue.
    fn submit(&self) -> Option<SubmitPlan> {
        Some(PASTE_AND_ENTER)
    }

    fn deny_flags(&self) -> &'static [&'static str] {
        &["--always-approve"]
    }
}
