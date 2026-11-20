//! GitHub Copilot CLI: what each of its hooks reports about its session.
//! Its hooks carry camelCase fields and fire a session's start at its
//! first prompt; the end of a turn carries no reply, which its transcript
//! holds. An interrupt or a declined permission fires no hook: its
//! terminal's OSC 7501 records end such a turn. Checked live against
//! Copilot CLI 1.0.114.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::Value;

use crate::agent::{
    Agent, Env, PASTE_AND_ENTER, StartPlan, StartRequest, SubmitPlan, resumable_id, spec,
};
use crate::hooks::{Detail, HookEvent, HookReport, StartSource, TurnEnd, WaitKind, text};
use crate::integration::{HookEntry, HookPlan, Layout};
use crate::registry::AgentSpec;

pub struct Copilot;

/// The most of a transcript's end read for the reply: Copilot writes a
/// line per event, and the reply is among the last.
const TRANSCRIPT_TAIL: u64 = 1024 * 1024;

/// How Copilot begins the prompts it gives itself rather than the person:
/// the notice that a background command finished starts a turn of its own.
const OWN_PROMPT: &str = "<system_notification>";

impl Agent for Copilot {
    fn spec(&self) -> &'static AgentSpec {
        spec("copilot")
    }

    fn parse_hook(&self, event: &str, input: &Value, _env: Env) -> Option<HookReport> {
        // a subagent's hook does not speak for the session; Copilot's hooks
        // carry no session id in the environment to check them against
        if input.get("agentId").is_some() {
            return None;
        }
        let session = text(input, "sessionId")?;
        let mut detail = Detail {
            transcript_path: text(input, "transcriptPath"),
            cwd: text(input, "cwd"),
            ..Detail::default()
        };
        let event = match event {
            // at the session's first prompt, not at launch
            "session-start" => HookEvent::Started {
                source: match text(input, "source").as_deref() {
                    Some("resume") => StartSource::Resume,
                    _ => StartSource::Startup,
                },
            },
            "prompt" => match text(input, "prompt") {
                Some(prompt) if prompt.starts_with(OWN_PROMPT) => HookEvent::Working,
                prompt => {
                    detail.prompt = prompt;
                    HookEvent::Prompt
                }
            },
            // the question tool waits for the person's answer, and is the
            // only report that says the question
            "tool" if text(input, "toolName").as_deref() == Some("ask_user") => {
                detail.summary = input
                    .pointer("/toolArgs/message")
                    .and_then(Value::as_str)
                    .map(|q| q.chars().take(200).collect());
                HookEvent::NeedsInput {
                    kind: WaitKind::Question,
                }
            }
            "tool" | "tool-done" => HookEvent::Working,
            "notification" => {
                let kind = match text(input, "notification_type")?.as_str() {
                    "permission_prompt" => WaitKind::Permission,
                    "elicitation_dialog" => WaitKind::Question,
                    _ => return None,
                };
                if kind == WaitKind::Permission {
                    detail.summary = text(input, "message").map(|m| m.chars().take(200).collect());
                }
                HookEvent::NeedsInput { kind }
            }
            "stop" => {
                detail.reply = detail
                    .transcript_path
                    .as_deref()
                    .and_then(|path| reply_once_written(Path::new(path)));
                HookEvent::TurnEnded { end: TurnEnd::Done }
            }
            "error" => {
                detail.error = input
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_string);
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

    /// A file of Pinrail's own in Copilot's hooks folder,
    /// `~/.copilot/hooks/` (or `$COPILOT_HOME/hooks/`), which Copilot loads
    /// beside the person's files: for the start of a session, each prompt,
    /// each tool call, a notification that it waits on the person, the end
    /// of each turn, an error, and the end of the session. Copilot's own
    /// event names: its tool events do not fire under Claude Code's.
    fn hook_plan(&self, home: &Path, env: Env) -> Option<HookPlan> {
        let entry = |event, hook| HookEntry::new("copilot", event, None, hook);
        let folder = env("COPILOT_HOME")
            .filter(|h| !h.trim().is_empty())
            .map(Into::into)
            .unwrap_or_else(|| home.join(".copilot"));
        Some(HookPlan {
            file: folder.join("hooks").join("pinrail.json"),
            layout: Layout::Flat,
            entries: vec![
                entry("sessionStart", "session-start"),
                entry("userPromptSubmitted", "prompt"),
                entry("preToolUse", "tool"),
                entry("postToolUse", "tool-done"),
                entry("notification", "notification"),
                entry("agentStop", "stop"),
                entry("errorOccurred", "error"),
                entry("sessionEnd", "session-end"),
            ],
        })
    }

    /// `copilot --session-id <uuid> [extra args] [--interactive=<task>]`:
    /// Pinrail chooses the id, so the session exists from the start. The
    /// task as one `=` argument is never read as a flag.
    fn start(&self, request: &StartRequest) -> Option<StartPlan> {
        let mut argv = vec![
            "copilot".to_string(),
            "--session-id".to_string(),
            request.session_id.to_string(),
        ];
        argv.extend(request.extra_args.iter().cloned());
        if let Some(task) = request.task.filter(|t| !t.trim().is_empty()) {
            argv.push(format!("--interactive={task}"));
        }
        Some(StartPlan {
            argv,
            session: Some(request.session_id.to_string()),
        })
    }

    /// `copilot --resume=<id>`. An id Copilot does not know exits at once.
    fn resume(&self, session: &str) -> Option<Vec<String>> {
        resumable_id(session).then(|| vec!["copilot".to_string(), format!("--resume={session}")])
    }

    /// A bracketed paste and Enter, which Copilot takes as one prompt with
    /// its line breaks, confirmed by `userPromptSubmitted`. Pinrail types a
    /// message once the agent is idle; Enter while it works would join the
    /// running turn instead.
    fn submit(&self) -> Option<SubmitPlan> {
        Some(PASTE_AND_ENTER)
    }

    fn deny_flags(&self) -> &'static [&'static str] {
        &[
            "--allow-all",
            "--allow-all-tools",
            "--allow-all-paths",
            "--allow-all-urls",
        ]
    }
}

/// How long the end of a turn waits for Copilot to write the turn's reply,
/// which it may not have written to the file when the hook runs, and how
/// often it looks again. Well within the hook's second.
const REPLY_WAIT: std::time::Duration = std::time::Duration::from_millis(500);
const REPLY_EVERY: std::time::Duration = std::time::Duration::from_millis(50);

/// The turn's reply, once Copilot has written it, or `None` after a short
/// wait.
fn reply_once_written(transcript: &Path) -> Option<String> {
    let started = std::time::Instant::now();
    loop {
        match reply(transcript) {
            Reply::Written(text) => return Some(text),
            Reply::Unreadable => return None,
            Reply::NotYet if started.elapsed() >= REPLY_WAIT => return None,
            Reply::NotYet => std::thread::sleep(REPLY_EVERY),
        }
    }
}

enum Reply {
    Written(String),
    NotYet,
    Unreadable,
}

/// The turn's reply, from the transcript Copilot keeps: the content of the
/// last `assistant.message` that has any, after the turn's prompt, the
/// last `user.message`; never an earlier turn's. The file is
/// undocumented, so anything unexpected gives no reply.
fn reply(transcript: &Path) -> Reply {
    let Some(text) = tail(transcript) else {
        return Reply::Unreadable;
    };
    for event in text
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        match event.get("type").and_then(Value::as_str) {
            Some("user.message") => return Reply::NotYet,
            Some("assistant.message") => {
                if let Some(content) = event
                    .pointer("/data/content")
                    .and_then(Value::as_str)
                    .filter(|c| !c.trim().is_empty())
                {
                    return Reply::Written(content.to_string());
                }
            }
            _ => {}
        }
    }
    Reply::NotYet
}

/// The end of the transcript, as text.
fn tail(transcript: &Path) -> Option<String> {
    let mut file = std::fs::File::open(transcript).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(TRANSCRIPT_TAIL)))
        .ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}
