//! What Pinrail does with an agent beyond the facts of its registry entry.
//! Each agent with behaviour of its own has an adapter in `agents/`; the
//! others are data only, and have none.

use std::path::Path;

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

    /// What the agent itself says of the session `session` held by the
    /// process `pid`, from a file it keeps under `home` (or `env`'s
    /// configuration folder), when it keeps one. A hint: `None` whenever
    /// the file is missing, names another session, or is not as expected.
    fn presence_hint(&self, _home: &Path, _env: Env, _pid: u32, _session: &str) -> Option<Hint> {
        None
    }

    /// Where the agent keeps its hooks under `home` (or where `env` moved
    /// its configuration), and the hooks Pinrail installs there so that it
    /// can show the agent's status; `None` for an agent whose hooks Pinrail
    /// does not install.
    fn hook_plan(&self, _home: &Path, _env: Env) -> Option<crate::integration::HookPlan> {
        None
    }

    /// How Pinrail starts a new session of the agent in a terminal; `None`
    /// for an agent Pinrail does not start.
    fn start(&self, _request: &StartRequest) -> Option<StartPlan> {
        None
    }

    /// The command that resumes the stopped session `session`; `None` for
    /// an agent Pinrail does not resume, or an id that is not a plain
    /// session id (see [`resumable_id`]).
    fn resume(&self, _session: &str) -> Option<Vec<String>> {
        None
    }

    /// Whether a session still running in another terminal can be resumed
    /// in a second process, both of them answering it. Claude Code allows
    /// it; the others are not yet known to.
    fn resumes_while_running(&self) -> bool {
        false
    }

    /// Flags that turn the agent's permission checks off, which Pinrail
    /// refuses in a person's extra arguments, beside the shared list.
    fn deny_flags(&self) -> &'static [&'static str] {
        &[]
    }
}

/// A new session Pinrail is asked to start.
#[derive(Debug, Clone, Copy)]
pub struct StartRequest<'a> {
    /// The first task, if the person gave one.
    pub task: Option<&'a str>,
    /// The person's extra arguments, already checked against the deny list.
    pub extra_args: &'a [String],
    /// A new UUID, the session's id for an agent that lets Pinrail choose it.
    pub session_id: &'a str,
}

/// How to start the agent: the command, and what Pinrail knows of the
/// session before it reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartPlan {
    pub argv: Vec<String>,
    /// The session's id, when the agent let Pinrail choose it; otherwise
    /// the run learns it from the agent's first `SessionStart`.
    pub session: Option<String>,
}

/// Adds the first task to a command as its last argument, after `--`, so
/// that a task that starts with `-` is never read as a flag.
pub(crate) fn push_task(argv: &mut Vec<String>, task: Option<&str>) {
    if let Some(task) = task.filter(|t| !t.trim().is_empty()) {
        argv.push("--".to_string());
        argv.push(task.to_string());
    }
}

/// Whether `id` can be given to an agent as a session id: letters,
/// digits, `-`, `_` and `.`, not starting with `-`. A session's id comes
/// from its hooks' reports, so an id that an agent could read as a flag,
/// or as more than one argument, is never passed on.
pub fn resumable_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Flags that turn permission checks off in more than one agent.
const DENIED_EVERYWHERE: &[&str] = &[
    "--dangerously-skip-permissions",
    "--allow-dangerously-skip-permissions",
    "--yolo",
];

/// The first of `args` that turns the agent's permission checks off, as
/// written. A flag counts alone or with `=value`, and `--permission-mode`
/// counts with the value `bypassPermissions`.
pub fn denied_arg<'a>(agent: &dyn Agent, args: &'a [String]) -> Option<&'a str> {
    let denied =
        |flag: &str| DENIED_EVERYWHERE.contains(&flag) || agent.deny_flags().contains(&flag);
    args.iter().enumerate().find_map(|(i, arg)| {
        let (flag, value) = match arg.split_once('=') {
            Some((flag, value)) => (flag, Some(value)),
            None => (arg.as_str(), args.get(i + 1).map(String::as_str)),
        };
        let bypass = flag == "--permission-mode" && value == Some("bypassPermissions");
        (denied(flag) || bypass).then_some(arg.as_str())
    })
}

/// What an agent's own file says its session is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hint {
    pub status: HintStatus,
    /// When the status last changed, in milliseconds since the epoch.
    pub since_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintStatus {
    /// It works on a turn.
    Busy,
    /// It waits for the person's next prompt.
    Idle,
    /// It waits for the person to answer something.
    Waiting,
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
