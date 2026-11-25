//! Codex: what each of its hooks reports about its session. The
//! fields are those of `codex-rs/hooks/schema/generated/`.

use std::path::Path;

use serde_json::Value;

use crate::agent::{
    Agent, Env, PASTE_AND_ENTER, StartPlan, StartRequest, SubmitPlan, push_task, resumable_id, spec,
};
use crate::hooks::{HookEvent, HookReport, TurnEnd, WaitKind, common, start_source, summary, text};
use crate::integration::{HookEntry, Integration, Kind, Layout, OwnFile};
use crate::registry::AgentSpec;

pub struct Codex;

/// Codex's rule for the `pinrail` command: run it outside the sandbox, which
/// turns the network off, so that it reaches the app. Codex applies a rule
/// only to a plain command, not to one in a pipe or a heredoc.
const PINRAIL_RULE: &str = "\
# Written by Pinrail. Codex runs the pinrail command outside its sandbox,
# so that the command can reach the Pinrail app.
prefix_rule(pattern=[\"pinrail\"], decision=\"allow\")
";

/// The start of the rule file's first line, which marks it as Pinrail's.
const RULE_MARKER: &str = "# Written by Pinrail.";

/// Codex's folder: `$CODEX_HOME`, or `~/.codex`.
fn codex_home(home: &Path, env: Env) -> std::path::PathBuf {
    env("CODEX_HOME")
        .filter(|h| !h.trim().is_empty())
        .map(Into::into)
        .unwrap_or_else(|| home.join(".codex"))
}

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
        Some(Integration {
            kind: Kind::Hooks,
            file: codex_home(home, env).join("hooks.json"),
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

    /// A rule file of Pinrail's own in Codex's rules folder, which lets the
    /// skill's `pinrail` commands out of Codex's sandbox.
    fn skill_files(&self, home: &Path, env: Env) -> Vec<OwnFile> {
        vec![OwnFile {
            path: codex_home(home, env).join("rules").join("pinrail.rules"),
            text: PINRAIL_RULE,
            marker: RULE_MARKER,
        }]
    }

    /// Through Codex's app server, started in the login shell, so that the
    /// person does not have to trust Pinrail's hooks at Codex's next start.
    fn trust_hooks(
        &self,
        plan: &Integration,
        shell: &dyn Fn(&str) -> std::process::Command,
    ) -> Result<usize, String> {
        super::codex_trust::trust(plan, shell("codex app-server"))
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
            type_task: None,
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

    /// `codex --no-daemon [extra args] resume <id>`: the person's arguments
    /// are Codex's own options, which go before its `resume` command.
    fn resume_with(&self, session: &str, extra: &[String]) -> Option<Vec<String>> {
        let mut argv = self.resume(session)?;
        let at = argv.iter().position(|a| a == "resume")?;
        argv.splice(at..at, extra.iter().cloned());
        Some(argv)
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::connection::SkillFiles;
    use crate::connection::testing::{Case, connects_and_removes, env, home_with, no_shell, skill};

    #[test]
    fn it_is_connected_and_removed_with_its_hooks_trusted() {
        let trust = tempfile::tempdir().unwrap();
        let out = trust.path().join("write.json");
        let case = Case {
            config: ".codex",
            skills: ".codex/skills",
            reporting: Some(".codex/hooks.json"),
            theirs: Some(
                r#"{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "my-own-stop"}]}]}}"#,
            ),
        };
        // connecting trusts the hooks through Codex's app server, run in
        // the login shell; codex_trust's tests cover what it says to it
        let started = std::cell::Cell::new(0);
        let shell = |line: &str| {
            assert_eq!(line, "codex app-server");
            started.set(started.get() + 1);
            super::super::codex_trust::fake_app_server(&out)
        };
        connects_and_removes(&Codex, &case, &shell);
        // once for each connect
        assert_eq!(started.get(), 2);
    }

    /// Codex's app server as the fake answers it, keeping what it was
    /// asked in a folder of the test's own.
    fn app_server(_: &str) -> std::process::Command {
        let out =
            std::env::temp_dir().join(format!("pinrail-codex-trust-{}.json", std::process::id()));
        super::super::codex_trust::fake_app_server(&out)
    }

    #[test]
    fn it_gets_a_rule_for_the_pinrail_command_with_its_skill() {
        let home = home_with(&[".codex"]);
        let home = home.path();
        let files = skill("abc");
        let skill = SkillFiles {
            files: &files,
            sha: "abc",
        };
        let rule = home.join(".codex/rules/pinrail.rules");
        Codex.connect(home, &env, &skill, &app_server).unwrap();
        let text = fs::read_to_string(&rule).unwrap();
        assert!(
            text.contains(r#"prefix_rule(pattern=["pinrail"], decision="allow")"#),
            "{text}"
        );
        // without its rule, the skill is outdated, and updating writes it
        fs::remove_file(&rule).unwrap();
        assert_eq!(
            Codex.skill_state(home, &env, "abc"),
            crate::connection::SkillState::Outdated
        );
        Codex.connect(home, &env, &skill, &app_server).unwrap();
        assert_eq!(
            Codex.skill_state(home, &env, "abc"),
            crate::connection::SkillState::Connected
        );

        Codex.disconnect(home, &env).unwrap();
        assert!(!rule.exists());
    }

    #[test]
    fn a_rule_file_of_the_person_s_own_is_left_alone() {
        let home = home_with(&[".codex/rules"]);
        let home = home.path();
        let rule = home.join(".codex/rules/pinrail.rules");
        let theirs = "prefix_rule(pattern=[\"pinrail\", \"status\"], decision=\"allow\")\n";
        fs::write(&rule, theirs).unwrap();
        let files = skill("abc");
        let skill = SkillFiles {
            files: &files,
            sha: "abc",
        };
        assert!(Codex.connect(home, &env, &skill, &no_shell).is_err());
        assert_eq!(fs::read_to_string(&rule).unwrap(), theirs);
        Codex.disconnect(home, &env).unwrap();
        assert_eq!(fs::read_to_string(&rule).unwrap(), theirs);
    }
}
