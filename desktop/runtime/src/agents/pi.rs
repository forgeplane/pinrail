//! Pi: what Pinrail's extension for Pi reports about its session. Pi has no
//! shell hooks; the extension, which Pinrail installs in Pi's extensions
//! folder, runs `pinrail hook pi <event>` for the events of Pi's extension
//! API, with the session's id and the turn's reply. Pi runs as `node`, so
//! the extension sends its process's id too. Checked live against Pi 1.5.2.

use std::path::Path;

use serde_json::Value;

use crate::agent::{
    Agent, Env, PASTE_AND_ENTER, StartPlan, StartRequest, SubmitPlan, resumable_id, spec,
};
use crate::hooks::{Detail, HookEvent, HookReport, StartSource, TurnEnd, WaitKind, text};
use crate::integration::{Integration, Kind, Layout};
use crate::process::Process;
use crate::registry::AgentSpec;

pub struct Pi;

/// The extension Pinrail installs, and the line that tells it from a file
/// of the person's.
const EXTENSION: &str = include_str!("pi-extension.ts");
const EXTENSION_MARKER: &str = "// Pinrail's extension for Pi";

impl Agent for Pi {
    fn spec(&self) -> &'static AgentSpec {
        spec("pi")
    }

    fn parse_hook(&self, event: &str, input: &Value, _env: Env) -> Option<HookReport> {
        let session = text(input, "session_id")?;
        let mut detail = Detail {
            transcript_path: text(input, "transcript_path"),
            cwd: text(input, "cwd"),
            ..Detail::default()
        };
        let event = match event {
            // at launch, and for the session /new starts in the same process;
            // a resume by flag says startup too
            "session-start" => HookEvent::Started {
                source: match text(input, "reason").as_deref() {
                    Some("resume") => StartSource::Resume,
                    Some("fork") => StartSource::Fork,
                    _ => StartSource::Startup,
                },
            },
            // a message sent while Pi works joins the running turn
            "prompt" if text(input, "streaming").is_some() => HookEvent::Working,
            "prompt" => {
                detail.prompt = text(input, "prompt");
                HookEvent::Prompt
            }
            "tool" | "answered" => HookEvent::Working,
            // a dialog of an extension: Pi has no permission checks of its
            // own, and calls a confirmation one
            "waiting" => {
                detail.summary = text(input, "title").map(|t| t.chars().take(200).collect());
                HookEvent::NeedsInput {
                    kind: match text(input, "kind").as_deref() {
                        Some("confirm") => WaitKind::Permission,
                        _ => WaitKind::Question,
                    },
                }
            }
            "stop" => {
                let end = if input.get("aborted").and_then(Value::as_bool) == Some(true) {
                    TurnEnd::Interrupted
                } else if text(input, "stop_reason").as_deref() == Some("error") {
                    detail.error = text(input, "error");
                    TurnEnd::Failed
                } else {
                    detail.reply = text(input, "reply");
                    TurnEnd::Done
                };
                HookEvent::TurnEnded { end }
            }
            // a session /new, a resume or a fork leaves ends with the run's
            // move to the next one
            "session-end" if text(input, "reason").as_deref() != Some("quit") => return None,
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

    /// Pinrail's extension, `pinrail.ts` in Pi's extensions folder,
    /// `~/.pi/agent/extensions/` (or `$PI_CODING_AGENT_DIR/extensions/`),
    /// which Pi loads at start beside the person's extensions.
    fn integration(&self, home: &Path, env: Env) -> Option<Integration> {
        let folder = env("PI_CODING_AGENT_DIR")
            .filter(|h| !h.trim().is_empty())
            .map(Into::into)
            .unwrap_or_else(|| home.join(".pi").join("agent"));
        Some(Integration {
            kind: Kind::Extension,
            file: folder.join("extensions").join("pinrail.ts"),
            layout: Layout::File {
                text: EXTENSION,
                marker: EXTENSION_MARKER,
            },
            entries: vec![],
        })
    }

    /// Pi runs as `node` (or `bun`) with `pi` as its first argument, or,
    /// built on its own, as `pi`.
    fn is_own_process(&self, process: &Process) -> bool {
        if process.name == "pi" {
            return true;
        }
        if !matches!(process.name.as_str(), "node" | "bun") {
            return false;
        }
        crate::process::arguments(process.pid)
            .and_then(|args| args.into_iter().next())
            .is_some_and(|first| {
                Path::new(&first)
                    .file_name()
                    .is_some_and(|name| name == "pi")
            })
    }

    /// `pi --session-id <id> [extra args] -- [task]`: Pinrail chooses the
    /// id, which Pi creates. Pi reads an argument that starts with `@` as a
    /// file to attach, even after `--`, so such a task goes with a space in
    /// front.
    fn start(&self, request: &StartRequest) -> Option<StartPlan> {
        let mut argv = vec![
            "pi".to_string(),
            "--session-id".to_string(),
            request.session_id.to_string(),
        ];
        argv.extend(request.extra_args.iter().cloned());
        if let Some(task) = request.task.filter(|t| !t.trim().is_empty()) {
            argv.push("--".to_string());
            argv.push(if task.starts_with('@') {
                format!(" {task}")
            } else {
                task.to_string()
            });
        }
        Some(StartPlan {
            argv,
            session: Some(request.session_id.to_string()),
        })
    }

    /// `pi --session <id>`, in the session's folder. An id Pi does not know
    /// exits at once; `--session-id` would create an empty session instead.
    fn resume(&self, session: &str) -> Option<Vec<String>> {
        resumable_id(session).then(|| {
            vec![
                "pi".to_string(),
                "--session".to_string(),
                session.to_string(),
            ]
        })
    }

    /// A bracketed paste and Enter, which Pi takes as one prompt with its
    /// line breaks, confirmed by the extension's prompt report.
    fn submit(&self) -> Option<SubmitPlan> {
        Some(PASTE_AND_ENTER)
    }

    /// Pi has no permission checks to turn off; `--approve` trusts a
    /// folder's own extensions, which then run unasked.
    fn deny_flags(&self) -> &'static [&'static str] {
        &["--approve", "-a"]
    }
}
