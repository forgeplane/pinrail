//! What Pinrail does with an agent beyond the facts of its registry entry.
//! Each agent with behaviour of its own has an adapter in `agents/`; the
//! others are data only, and have none.

use serde_json::Value;

use crate::agents::{ClaudeCode, Codex};
use crate::hooks::HookReport;
use crate::registry::AgentSpec;

/// A variable of the environment the hook runs in, by name.
pub type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

pub trait Agent: Send + Sync {
    fn spec(&self) -> &'static AgentSpec;

    /// What the hook `event` (Pinrail's name for it, such as `prompt`)
    /// reports, from the agent's JSON input. `None` when it reports nothing
    /// about the session: an event Pinrail does not use, a subagent's hook,
    /// or the hook of another agent that runs this one's hooks.
    fn parse_hook(&self, event: &str, input: &Value, env: Env) -> Option<HookReport>;
}

/// The adapter of the agent an id or an alias names, if it has one.
pub fn adapter(name: &str) -> Option<&'static dyn Agent> {
    let agent: &'static dyn Agent = match crate::registry::find(name)?.id {
        "claude-code" => &ClaudeCode,
        "codex" => &Codex,
        _ => return None,
    };
    Some(agent)
}

/// The registry entry of an adapter's agent.
pub(crate) fn spec(id: &str) -> &'static AgentSpec {
    crate::registry::find(id).expect("an adapter's agent is in the registry")
}
