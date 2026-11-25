//! OpenCode: what Pinrail's plugin for OpenCode reports about the session on
//! screen. OpenCode has no shell hooks and writes no OSC 7501; its terminal
//! interface loads plugins, and Pinrail's turns the events it gets into
//! reports by running `pinrail hook opencode <event>`, with OpenCode's
//! process's id. Checked live against OpenCode 2.0.20.

use std::path::Path;

use serde_json::Value;

use crate::agent::{
    Agent, Env, PASTE_AND_ENTER, StartPlan, StartRequest, SubmitPlan, resumable_id, spec,
};
use crate::hooks::{Detail, HookEvent, HookReport, StartSource, TurnEnd, WaitKind, text};
use crate::integration::{Integration, Kind, Layout};
use crate::registry::AgentSpec;

pub struct OpenCode;

/// The plugin Pinrail installs, its entry in the plugin list, and the line
/// that tells it from a file of the person's.
const PLUGIN: &str = include_str!("opencode-plugin.js");
const PLUGIN_MARKER: &str = "// Pinrail's plugin for OpenCode";

impl Agent for OpenCode {
    fn spec(&self) -> &'static AgentSpec {
        spec("opencode")
    }

    fn parse_hook(&self, event: &str, input: &Value, _env: Env) -> Option<HookReport> {
        let session = text(input, "session_id")?;
        let mut detail = Detail {
            cwd: text(input, "cwd"),
            ..Detail::default()
        };
        let event = match event {
            "session-start" => HookEvent::Started {
                source: match text(input, "reason").as_deref() {
                    Some("resume") => StartSource::Resume,
                    _ => StartSource::Startup,
                },
            },
            // a message sent while OpenCode works joins the running execution
            "prompt" if input.get("working").and_then(Value::as_bool) == Some(true) => {
                HookEvent::Working
            }
            "prompt" => {
                detail.prompt = text(input, "prompt");
                HookEvent::Prompt
            }
            "tool" | "answered" => HookEvent::Working,
            "waiting" => {
                detail.summary = text(input, "summary").map(|s| s.chars().take(200).collect());
                HookEvent::NeedsInput {
                    kind: match text(input, "kind").as_deref() {
                        Some("question") => WaitKind::Question,
                        _ => WaitKind::Permission,
                    },
                }
            }
            "stop" => {
                let end = match text(input, "end").as_deref() {
                    Some("failed") => {
                        detail.error = text(input, "error");
                        TurnEnd::Failed
                    }
                    Some("interrupted") => TurnEnd::Interrupted,
                    _ => {
                        detail.reply = text(input, "reply");
                        TurnEnd::Done
                    }
                };
                HookEvent::TurnEnded { end }
            }
            "session-end" => HookEvent::Ended,
            _ => return None,
        };
        Some(HookReport {
            session,
            event,
            detail,
        })
    }

    /// Pinrail's plugin, `pinrail-opencode/tui.js` in OpenCode's
    /// configuration folder, `~/.config/opencode/` (or under
    /// `$XDG_CONFIG_HOME`), and its entry in the `plugins` list of that
    /// folder's `cli.json`, beside the person's.
    fn integration(&self, home: &Path, env: Env) -> Option<Integration> {
        let config = env("XDG_CONFIG_HOME")
            .filter(|h| !h.trim().is_empty())
            .map(Into::into)
            .unwrap_or_else(|| home.join(".config"));
        Some(Integration {
            kind: Kind::Plugin,
            file: config.join("opencode").join("cli.json"),
            layout: Layout::Listed {
                key: "plugins",
                entry: "./pinrail-opencode",
                path: "pinrail-opencode/tui.js",
                text: PLUGIN,
                marker: PLUGIN_MARKER,
            },
            entries: vec![],
        })
    }

    /// `opencode --standalone --session ses_<id> [extra args]
    /// [--prompt=<task>]`: an id OpenCode does not know is created with
    /// it at the first prompt, so Pinrail chooses it, in OpenCode's form,
    /// which starts with `ses`. Its own server runs inside the terminal's
    /// process, with the run's environment, not in the background service
    /// another terminal may have started.
    fn start(&self, request: &StartRequest) -> Option<StartPlan> {
        let session = format!("ses_{}", request.session_id.replace('-', ""));
        let mut argv = vec![
            "opencode".to_string(),
            "--standalone".to_string(),
            "--session".to_string(),
            session.clone(),
        ];
        argv.extend(request.extra_args.iter().cloned());
        if let Some(task) = request.task.filter(|t| !t.trim().is_empty()) {
            argv.push(format!("--prompt={task}"));
        }
        Some(StartPlan {
            argv,
            session: Some(session),
            type_task: None,
        })
    }

    /// `opencode --standalone --session <id>`.
    fn resume(&self, session: &str) -> Option<Vec<String>> {
        resumable_id(session).then(|| {
            ["opencode", "--standalone", "--session", session]
                .map(str::to_string)
                .to_vec()
        })
    }

    /// A bracketed paste and Enter, which OpenCode takes as one prompt with
    /// its line breaks, confirmed by the plugin's prompt report.
    fn submit(&self) -> Option<SubmitPlan> {
        Some(PASTE_AND_ENTER)
    }

    fn deny_flags(&self) -> &'static [&'static str] {
        &["--auto"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::testing::{Case, connects_and_removes, no_shell};

    #[test]
    fn it_is_connected_and_removed_with_its_plugin_beside_the_person_s() {
        let case = Case {
            config: ".config/opencode",
            skills: ".config/opencode/skills",
            reporting: Some(".config/opencode/cli.json"),
            theirs: Some(r#"{"plugins": ["./mine"], "theme": "dark"}"#),
        };
        connects_and_removes(&OpenCode, &case, &no_shell);
    }
}
