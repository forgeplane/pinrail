//! A stand-in agent for the core's tests, built only with the `stand-in`
//! feature: it runs as `claude`, reports as Claude Code does, and takes no
//! first task in its command, so that Pinrail types it.

use serde_json::Value;

use crate::agent::{Agent, Env, PASTE_AND_ENTER, StartPlan, StartRequest, SubmitPlan};
use crate::agents::ClaudeCode;
use crate::hooks::HookReport;
use crate::registry::AgentSpec;

pub struct StandIn;

/// The stand-in's entry, outside the registry, which lists real agents.
pub(crate) static SPEC: AgentSpec = AgentSpec {
    id: "stand-in",
    aliases: &[],
    label: "Stand-in",
    binaries: &["claude"],
    markers: &[],
    session_var: None,
    folders: None,
};

impl Agent for StandIn {
    fn spec(&self) -> &'static AgentSpec {
        &SPEC
    }

    fn parse_hook(&self, event: &str, input: &Value, env: Env) -> Option<HookReport> {
        ClaudeCode.parse_hook(event, input, env)
    }

    /// `claude [extra args]`, with the task left out to be typed.
    fn start(&self, request: &StartRequest) -> Option<StartPlan> {
        let mut argv = vec!["claude".to_string()];
        argv.extend(request.extra_args.iter().cloned());
        Some(StartPlan {
            argv,
            session: None,
            type_task: request
                .task
                .filter(|t| !t.trim().is_empty())
                .map(str::to_string),
        })
    }

    fn submit(&self) -> Option<SubmitPlan> {
        Some(PASTE_AND_ENTER)
    }
}
