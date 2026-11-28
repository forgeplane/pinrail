//! Claude Code: what each of its hooks reports about its session.

use serde_json::Value;

use std::path::Path;

use crate::agent::{
    Agent, Env, Hint, HintStatus, PASTE_AND_ENTER, StartPlan, StartRequest, SubmitPlan,
    TranscriptTurn, push_task, resumable_id, spec,
};
use crate::hooks::{HookEvent, HookReport, TurnEnd, WaitKind, common, start_source, summary, text};
use crate::integration::{HookEntry, Integration, Kind, Layout};
use crate::registry::AgentSpec;

pub struct ClaudeCode;

/// How Claude Code begins the prompts it gives itself rather than the
/// person: a subagent's result handed back to the main session, the
/// notice that a background command finished, and a message from another
/// Claude Code session. Each fires `UserPromptSubmit` without an
/// `agent_id`, but the turn goes on.
const OWN_PROMPTS: [&str; 3] = [
    "<agent-message ",
    "<task-notification>",
    "<cross-session-message ",
];

/// A folder as Claude Code matches paths in its rules: on Windows in POSIX
/// form, `C:\Users\p` as `/c/Users/p`; elsewhere as it is.
fn posix(folder: &str) -> String {
    // a share, in its long form too: \\server\share, //server/share
    if let Some(share) = folder.strip_prefix(r"\\?\UNC\") {
        return format!("//{}", share.replace('\\', "/"));
    }
    let folder = folder.strip_prefix(r"\\?\").unwrap_or(folder);
    if folder.starts_with(r"\\") {
        return folder.replace('\\', "/");
    }
    let bytes = folder.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        return format!("/{drive}{}", folder[2..].replace('\\', "/"));
    }
    folder.to_string()
}

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
                if prompt
                    .as_deref()
                    .is_some_and(|p| OWN_PROMPTS.iter().any(|own| p.starts_with(own)))
                {
                    HookEvent::Working
                } else {
                    detail.prompt = prompt
                        .map(|p| without_paste_tags(&p))
                        .filter(|p| !p.is_empty());
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
                // the error as Claude Code shows it, then its details, then
                // only its type, such as `server_error`
                detail.error = text(input, "last_assistant_message")
                    .map(|shown| match shown.strip_prefix("API Error: ") {
                        Some(rest) => rest.to_string(),
                        None => shown,
                    })
                    .or_else(|| text(input, "error_details"))
                    .or_else(|| text(input, "error"));
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
    fn integration(&self, home: &Path, env: Env) -> Option<Integration> {
        let entry = |event, matcher, hook| HookEntry::new("claude-code", event, matcher, hook);
        Some(Integration {
            kind: Kind::Hooks,
            file: config_folder(home, env).join("settings.json"),
            layout: Layout::Settings,
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

    /// `claude --session-id <uuid> [extra args] [task]`: Pinrail chooses the
    /// id, so the session exists from the start.
    fn start(&self, request: &StartRequest) -> Option<StartPlan> {
        let mut argv = vec![
            "claude".to_string(),
            "--session-id".to_string(),
            request.session_id.to_string(),
        ];
        argv.extend(request.extra_args.iter().cloned());
        push_task(&mut argv, request.task);
        Some(StartPlan {
            argv,
            session: Some(request.session_id.to_string()),
            type_task: None,
        })
    }

    /// `claude --resume <id>`.
    fn resume(&self, session: &str) -> Option<Vec<String>> {
        resumable_id(session).then(|| {
            vec![
                "claude".to_string(),
                "--resume".to_string(),
                session.to_string(),
            ]
        })
    }

    /// Two Claude Code processes can hold one session, each adding to it.
    fn resumes_while_running(&self) -> bool {
        true
    }

    /// A bracketed paste and Enter, confirmed by `UserPromptSubmit`.
    fn submit(&self) -> Option<SubmitPlan> {
        Some(PASTE_AND_ENTER)
    }

    /// A rule that allows reading the folder, given with `--settings`.
    /// `--add-dir` would ask the person to trust the folder in every
    /// session, and would let Claude Code edit and run files there too.
    fn read_folder_args(&self, folder: &str) -> Vec<String> {
        // an absolute path in a rule starts with `//`
        let rule = format!("Read(/{}/**)", posix(folder).trim_end_matches('/'));
        let settings = serde_json::json!({ "permissions": { "allow": [rule] } });
        vec!["--settings".to_string(), settings.to_string()]
    }

    /// The native install links `claude` to
    /// `~/.local/share/claude/versions/<version>`, and the system names the
    /// process after that file: `2.1.331`.
    fn is_own_process(&self, process: &crate::process::Process) -> bool {
        let name = process.name.as_str();
        name == "claude"
            || (name.contains('.')
                && name.starts_with(|c: char| c.is_ascii_digit())
                && name
                    .split('.')
                    .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())))
    }

    fn skip_checks_flags(&self) -> &'static [&'static str] {
        &[
            "--dangerously-skip-permissions",
            "--allow-dangerously-skip-permissions",
            "--permission-mode=bypassPermissions",
        ]
    }

    /// An example of the person's arguments, from its `--help`.
    fn args_example(&self) -> Option<&'static str> {
        Some("--model opus")
    }

    /// The flags Pinrail sets to name, resume or prompt the session.
    fn own_flags(&self) -> &'static [&'static str] {
        &["--session-id", "--resume", "-r", "--continue", "-c"]
    }

    fn transcript_turn(&self, transcript: &Path) -> Option<TranscriptTurn> {
        last_turn(&tail(transcript, TRANSCRIPT_TAIL)?)
    }
}

/// The most of a transcript's end read for its latest turn: the reply is
/// among the last lines, and the prompt before a turn's tool calls.
const TRANSCRIPT_TAIL: u64 = 8 * 1024 * 1024;

/// The last `max` bytes of the file at `path`, from the first whole line.
fn tail(path: &Path, max: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let from = length.saturating_sub(max);
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    // a line cut by the start of the read is no line
    Some(match (from > 0, text.find('\n')) {
        (true, Some(at)) => text[at + 1..].to_string(),
        _ => text,
    })
}

/// The latest turn of Claude Code's transcript lines: the turn of the last
/// entry of the main conversation that names a prompt, the person's prompt
/// when the lines hold it, and the text of the agent's last message after
/// it. Subagents' entries, meta entries and tool results are not the
/// person's or the agent's words.
fn last_turn(lines: &str) -> Option<TranscriptTurn> {
    let mut turn: Option<TranscriptTurn> = None;
    for entry in lines
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
    {
        if entry.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let content = &entry["message"]["content"];
        match entry.get("type").and_then(Value::as_str) {
            Some("user") => {
                let Some(id) = text(&entry, "promptId") else {
                    continue;
                };
                let origin = entry["origin"]["kind"].as_str();
                let human = origin == Some("human")
                    && entry.get("isMeta").and_then(Value::as_bool) != Some(true);
                if human {
                    turn = Some(TranscriptTurn {
                        agent_turn: Some(id),
                        prompt: words(content).map(|p| without_paste_tags(&p)),
                        prompted_at: text(&entry, "timestamp"),
                        reply: None,
                        unprompted: false,
                    });
                } else if turn.as_ref().and_then(|t| t.agent_turn.as_deref()) != Some(id.as_str()) {
                    // a turn whose prompt is before the part read, or one
                    // the agent started itself: a tool result has no origin,
                    // a task notification or another session's message has
                    turn = Some(TranscriptTurn {
                        agent_turn: Some(id),
                        prompted_at: origin.and(text(&entry, "timestamp")),
                        unprompted: origin.is_some(),
                        ..TranscriptTurn::default()
                    });
                }
            }
            Some("assistant") => {
                if let (Some(turn), Some(said)) = (turn.as_mut(), words(content)) {
                    turn.reply = Some(said);
                }
            }
            _ => {}
        }
    }
    turn
}

/// A prompt without the tags Claude Code puts around text pasted into it,
/// which it also does for a message Pinrail types: a line
/// `<pasted_content id="…">` and, later, `</pasted_content id="…">` with
/// the same id of four hex digits. Claude Code does this for a paste of
/// more than 800 characters or 3 lines, which it shows as `[Pasted text #N]`. The pasted text stays in its place. Only the outermost
/// pairs are removed, so tags quoted inside a paste stay, and a tag without
/// its partner is the person's text.
fn without_paste_tags(text: &str) -> String {
    let opening = |line: &str| {
        line.trim()
            .strip_prefix("<pasted_content id=\"")?
            .strip_suffix("\">")
            // Claude Code's ids are four hex digits
            .filter(|id| id.len() == 4 && id.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(str::to_string)
    };
    let lines: Vec<&str> = text.split('\n').collect();
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    let mut removed = false;
    let mut at = 0;
    while at < lines.len() {
        let closing = opening(lines[at]).and_then(|id| {
            let closing = format!("</pasted_content id=\"{id}\">");
            (at + 1..lines.len()).find(|&end| lines[end].trim() == closing)
        });
        match closing {
            Some(end) => {
                kept.extend(&lines[at + 1..end]);
                removed = true;
                at = end + 1;
            }
            None => {
                kept.push(lines[at]);
                at += 1;
            }
        }
    }
    let text = kept.join("\n");
    // the blank lines Claude Code puts before a paste that begins the prompt
    if removed {
        text.trim().to_string()
    } else {
        text
    }
}

/// The text of a message's content: a string, or its text blocks joined.
fn words(content: &Value) -> Option<String> {
    let text = match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b["type"].as_str() == Some("text"))
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n\n"),
        _ => return None,
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_on_a_share_is_written_with_forward_slashes() {
        assert_eq!(posix(r"\\server\share\sessions"), "//server/share/sessions");
        assert_eq!(
            posix(r"\\?\UNC\server\share\sessions"),
            "//server/share/sessions"
        );
        assert_eq!(posix(r"C:\Users\me"), "/c/Users/me");
    }

    fn line(entry: Value) -> String {
        format!("{entry}\n")
    }

    fn prompt(id: &str, content: Value) -> String {
        line(
            serde_json::json!({"type": "user", "promptId": id, "origin": {"kind": "human"},
            "isSidechain": false, "timestamp": "2026-11-25T17:44:06.242Z",
            "message": {"role": "user", "content": content}}),
        )
    }

    fn said(content: Value) -> String {
        line(
            serde_json::json!({"type": "assistant", "isSidechain": false,
            "message": {"role": "assistant", "content": content}}),
        )
    }

    #[test]
    fn a_prompt_that_is_one_paste_is_its_text() {
        assert_eq!(
            without_paste_tags(
                "\n\n<pasted_content id=\"7620\">\nline one\nline two\n</pasted_content id=\"7620\">"
            ),
            "line one\nline two"
        );
    }

    #[test]
    fn typed_text_around_a_paste_stays_in_its_place() {
        let prompt = "look at this\n\n<pasted_content id=\"dccc\">\nFAIL pinrail-host\n</pasted_content id=\"dccc\">\n\nwhat do you think?";
        assert_eq!(
            without_paste_tags(prompt),
            "look at this\n\nFAIL pinrail-host\n\nwhat do you think?"
        );
    }

    #[test]
    fn paste_tags_quoted_inside_a_paste_are_kept() {
        let prompt = "<pasted_content id=\"00bb\">\nthe prompt was:\n<pasted_content id=\"00aa\">\nhi\n</pasted_content id=\"00aa\">\n</pasted_content id=\"00bb\">";
        assert_eq!(
            without_paste_tags(prompt),
            "the prompt was:\n<pasted_content id=\"00aa\">\nhi\n</pasted_content id=\"00aa\">"
        );
    }

    #[test]
    fn an_opening_paste_tag_without_its_closing_tag_is_kept() {
        let prompt = "<pasted_content id=\"00aa\">\nhi\n</pasted_content id=\"00bb\">";
        assert_eq!(without_paste_tags(prompt), prompt);
    }

    #[test]
    fn tags_with_an_id_claude_code_does_not_use_are_kept() {
        let prompt = "<pasted_content id=\"notes\">\nhi\n</pasted_content id=\"notes\">";
        assert_eq!(without_paste_tags(prompt), prompt);
    }

    #[test]
    fn a_pasted_prompt_in_the_transcript_is_its_text() {
        let lines = prompt(
            "p1",
            serde_json::json!(
                "\n\n<pasted_content id=\"7620\">\nFix it\n</pasted_content id=\"7620\">"
            ),
        );
        assert_eq!(last_turn(&lines).unwrap().prompt.as_deref(), Some("Fix it"));
    }

    #[test]
    fn its_transcript_gives_the_last_turn_with_its_prompt_and_last_reply() {
        let lines = [
            prompt("p1", serde_json::json!("First")),
            said(serde_json::json!([{"type": "text", "text": "First reply"}])),
            prompt(
                "p2",
                serde_json::json!([{"type": "image", "source": {}}, {"type": "text", "text": "Look at this"}]),
            ),
            // what Claude Code adds to a prompt, and a subagent's words
            line(serde_json::json!({"type": "user", "promptId": "p2", "isMeta": true,
                "message": {"content": [{"type": "text", "text": "[Image #1]"}]}})),
            line(serde_json::json!({"type": "assistant", "isSidechain": true,
                "message": {"content": [{"type": "text", "text": "A subagent's answer"}]}})),
            said(serde_json::json!([{"type": "thinking", "thinking": ""}, {"type": "tool_use", "id": "t"}])),
            line(serde_json::json!({"type": "user", "promptId": "p2",
                "message": {"content": [{"type": "tool_result", "tool_use_id": "t"}]}})),
            said(serde_json::json!([{"type": "text", "text": "Checked it."}])),
            said(serde_json::json!([{"type": "text", "text": "It looks right."}])),
        ]
        .concat();
        assert_eq!(
            last_turn(&lines),
            Some(TranscriptTurn {
                agent_turn: Some("p2".into()),
                prompt: Some("Look at this".into()),
                prompted_at: Some("2026-11-25T17:44:06.242Z".into()),
                reply: Some("It looks right.".into()),
                unprompted: false,
            })
        );
    }

    #[test]
    fn a_turn_started_by_a_task_notification_is_unprompted() {
        let lines = [
            prompt("p1", serde_json::json!("Run the suite in the background")),
            said(serde_json::json!([{"type": "text", "text": "Started it."}])),
            line(serde_json::json!({"type": "user", "promptId": "p2",
                "origin": {"kind": "task-notification"}, "timestamp": "2026-11-27T23:08:16.132Z",
                "message": {"content": "<task-notification>done</task-notification>"}})),
        ]
        .concat();
        let turn = last_turn(&lines).unwrap();
        assert_eq!(turn.agent_turn.as_deref(), Some("p2"));
        assert_eq!(turn.prompt, None);
        assert_eq!(
            turn.prompted_at.as_deref(),
            Some("2026-11-27T23:08:16.132Z")
        );
        assert!(turn.unprompted);
    }

    #[test]
    fn a_turn_whose_prompt_was_not_read_is_named_by_its_tool_results() {
        let lines = [
            line(serde_json::json!({"type": "user", "promptId": "p9",
                "message": {"content": [{"type": "tool_result", "tool_use_id": "t"}]}})),
            said(serde_json::json!([{"type": "text", "text": "Done."}])),
        ]
        .concat();
        let turn = last_turn(&lines).unwrap();
        assert_eq!(turn.agent_turn.as_deref(), Some("p9"));
        assert_eq!(turn.prompt, None);
        assert!(!turn.unprompted);
        assert_eq!(turn.reply.as_deref(), Some("Done."));
    }

    #[test]
    fn the_end_of_a_long_transcript_starts_at_a_whole_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let body = [
            prompt("p1", serde_json::json!("x".repeat(200))),
            prompt("p2", serde_json::json!("Second")),
            said(serde_json::json!([{"type": "text", "text": "Reply"}])),
        ]
        .concat();
        std::fs::write(&path, &body).unwrap();
        // the read starts inside the first prompt, which is dropped
        let turn = last_turn(&tail(&path, body.len() as u64 - 50).unwrap()).unwrap();
        assert_eq!(turn.prompt.as_deref(), Some("Second"));
        assert_eq!(
            ClaudeCode.transcript_turn(&path).unwrap().reply.as_deref(),
            Some("Reply")
        );
    }

    #[test]
    fn it_knows_the_flags_that_turn_its_checks_off() {
        crate::agent::testing::recognizes(
            &ClaudeCode,
            &[
                (
                    &["--dangerously-skip-permissions"],
                    "--dangerously-skip-permissions",
                ),
                (
                    &["--allow-dangerously-skip-permissions"],
                    "--allow-dangerously-skip-permissions",
                ),
                (
                    &["--permission-mode", "bypassPermissions"],
                    "--permission-mode",
                ),
                (
                    &["--permission-mode=bypassPermissions"],
                    "--permission-mode=bypassPermissions",
                ),
            ],
            &[
                &["--permission-mode", "plan", "--model", "opus"],
                &["--yolo"],
            ],
        );
    }
    use crate::connection::testing::{Case, connects_and_removes, no_shell};

    #[test]
    fn it_is_connected_and_removed_with_its_hooks() {
        let case = Case {
            config: ".claude",
            skills: ".claude/skills",
            reporting: Some(".claude/settings.json"),
            theirs: Some(
                r#"{"model": "opus", "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "my-own-stop"}]}]}}"#,
            ),
        };
        connects_and_removes(&ClaudeCode, &case, &no_shell);
    }
}
