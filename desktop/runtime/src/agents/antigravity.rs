//! Antigravity CLI (`agy`): what each of its hooks reports about its
//! conversation. Its hooks carry camelCase fields, start a conversation at
//! its first prompt rather than at launch, and carry no reply, which its
//! transcript holds. Checked live against agy 1.5.12.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::Value;

use crate::agent::{
    Agent, Env, PASTE_AND_ENTER, StartPlan, StartRequest, SubmitPlan, resumable_id, spec,
};
use crate::hooks::{Detail, HookEvent, HookReport, StartSource, TurnEnd, text};
use crate::integration::{HookEntry, Integration, Kind, Layout};
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
        // environment, and a subagent never receives a user prompt so its
        // lastUserInput is empty on every hook
        if env("ANTIGRAVITY_CONVERSATION_ID").as_deref() != Some(session.as_str())
            || text(input, "lastUserInput").is_none()
        {
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

    /// Earlier versions of Pinrail wrote the skill under `~/.gemini/config/skills`.
    fn legacy_skill_paths(&self, home: &Path) -> Vec<std::path::PathBuf> {
        vec![home.join(".gemini/config/skills/pinrail/SKILL.md")]
    }

    /// A hook named `pinrail` in agy's global hooks file,
    /// `~/.gemini/config/hooks.json`, beside the person's own: for the
    /// start of a conversation, each model call, each tool call that ends,
    /// and the end of each turn. agy has no hook for a permission or a
    /// question: its terminal's OSC 7501 records say when it waits.
    fn integration(&self, home: &Path, _env: Env) -> Option<Integration> {
        let entry = |event, matcher, hook| HookEntry::new("antigravity", event, matcher, hook);
        Some(Integration {
            kind: Kind::Hooks,
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

    /// `agy [extra args] [--prompt-interactive=<task>]`: agy ignores a bare
    /// argument, and the task as one `=` argument is never read as a flag.
    /// agy chooses the conversation's id, which the run learns from its
    /// first `SessionStart`.
    fn start(&self, request: &StartRequest) -> Option<StartPlan> {
        let mut argv = vec!["agy".to_string()];
        argv.extend(request.extra_args.iter().cloned());
        if let Some(task) = request.task.filter(|t| !t.trim().is_empty()) {
            argv.push(format!("--prompt-interactive={task}"));
        }
        Some(StartPlan {
            argv,
            session: None,
            type_task: None,
        })
    }

    /// `agy --conversation=<id>`. An id agy does not know starts a new
    /// conversation, whose first `SessionStart` moves the run to it.
    fn resume(&self, session: &str) -> Option<Vec<String>> {
        resumable_id(session).then(|| vec!["agy".to_string(), format!("--conversation={session}")])
    }

    /// The flags Pinrail sets to name, resume or prompt the session.
    fn own_flags(&self) -> &'static [&'static str] {
        &["--conversation", "--prompt-interactive"]
    }

    /// The flag that turns its permission checks off, from its `--help`.
    fn skip_checks_flags(&self) -> &'static [&'static str] {
        &["--dangerously-skip-permissions"]
    }

    /// An example of the person's arguments, from its `--help`.
    fn args_example(&self) -> Option<&'static str> {
        Some("--sandbox")
    }

    /// A bracketed paste and Enter, which agy takes as one prompt with its
    /// line breaks; the turn's first `PreInvocation` carries it.
    fn submit(&self) -> Option<SubmitPlan> {
        Some(SubmitPlan {
            confirms: Some("invocation"),
            ..PASTE_AND_ENTER
        })
    }

    /// Resuming or switching to an existing conversation fires no session
    /// start: its first prompt is the first sign.
    fn prompt_may_start_session(&self) -> bool {
        true
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_knows_the_flags_that_turn_its_checks_off() {
        crate::agent::testing::recognizes(
            &Antigravity,
            &[(
                &["--dangerously-skip-permissions"],
                "--dangerously-skip-permissions",
            )],
            &[&["--sandbox"]],
        );
    }
    use crate::connection::testing::{
        Case, assert_skill_present, connect, connects_and_removes, env, home_with, no_shell, skill,
    };

    #[test]
    fn it_is_connected_and_removed_with_its_named_hook() {
        let case = Case {
            config: ".gemini",
            skills: ".gemini/antigravity-cli/skills",
            reporting: Some(".gemini/config/hooks.json"),
            theirs: Some(
                r#"{"mine": {"PreInvocation": [{"command": "my-own-start", "type": "command"}]}}"#,
            ),
        };
        connects_and_removes(&Antigravity, &case, &no_shell);
    }

    #[test]
    fn connecting_removes_pinrails_skill_from_the_old_folder_first() {
        let home = home_with(&[".gemini"]);
        let home = home.path();
        let old = home.join(".gemini/config/skills/pinrail");
        for (rel, bytes) in skill("older") {
            let file = old.join(rel);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, bytes).unwrap();
        }

        connect(&Antigravity, home, &no_shell).unwrap();

        assert!(!old.exists(), "the old pinrail skill folder is removed");
        assert!(
            old.parent().unwrap().is_dir(),
            "the old skills folder itself stays"
        );
        assert_skill_present(&Antigravity, home);
    }

    #[test]
    fn connecting_leaves_the_persons_own_skill_in_the_old_folder_alone() {
        let home = home_with(&[".gemini"]);
        let home = home.path();
        let old = home.join(".gemini/config/skills/pinrail/SKILL.md");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        let theirs = "---\nname: pinrail\n---\nMy own skill.\n";
        std::fs::write(&old, theirs).unwrap();

        connect(&Antigravity, home, &no_shell).unwrap();

        assert_eq!(std::fs::read_to_string(&old).unwrap(), theirs);
        assert_skill_present(&Antigravity, home);
    }

    #[test]
    fn disconnecting_also_removes_pinrails_skill_from_the_old_folder() {
        let home = home_with(&[".gemini"]);
        let home = home.path();
        let old = home.join(".gemini/config/skills/pinrail");
        for (rel, bytes) in skill("older") {
            let file = old.join(rel);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, bytes).unwrap();
        }

        Antigravity.disconnect(home, &env).unwrap();

        assert!(!old.exists());
    }
}
