//! What Pinrail does with an agent beyond the facts of its registry entry.
//! Each agent with behaviour of its own has an adapter in `agents/`; the
//! others are data only, and have none.

use std::path::Path;

use serde_json::Value;

use crate::agents::{Antigravity, ClaudeCode, Codex, Copilot, Cursor, Grok, OpenCode, Pi};
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
    fn integration(&self, _home: &Path, _env: Env) -> Option<crate::integration::Integration> {
        None
    }

    /// Files of Pinrail's own that go with its skill under `home` (or where
    /// `env` moved the agent's configuration), such as a rule that lets the
    /// skill's `pinrail` commands reach the app from the agent's sandbox.
    fn skill_files(&self, _home: &Path, _env: Env) -> Vec<crate::integration::OwnFile> {
        Vec::new()
    }

    /// Trusts the hooks `plan` installed, for an agent that runs a hook only
    /// once the person has trusted it, and returns how many it trusted.
    /// `shell` makes the command that runs a command line, such as
    /// `codex app-server`, in the person's login shell, which finds the
    /// agent's program as a terminal would.
    fn trust_hooks(
        &self,
        _plan: &crate::integration::Integration,
        _shell: &dyn Fn(&str) -> std::process::Command,
    ) -> Result<usize, String> {
        Ok(0)
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

    /// How Pinrail types a message into the agent's terminal as a prompt;
    /// `None` for an agent it does not type into.
    fn submit(&self) -> Option<SubmitPlan> {
        None
    }

    /// Whether a session still running in another terminal can be resumed
    /// in a second process, both of them answering it. Claude Code allows
    /// it; the others are not yet known to.
    fn resumes_while_running(&self) -> bool {
        false
    }

    /// Variables set in the environment of the agent's runs, over the
    /// terminal's own: Cursor sends its notifications only to a terminal it
    /// recognises by `TERM_PROGRAM`.
    fn run_env(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    /// What the agent waits for, when a desktop notification of its
    /// terminal (OSC 9 or 777) says it and no hook does, with what it waits
    /// on: Cursor asks a permission only this way. `None` for any other
    /// notification.
    fn notification(
        &self,
        _title: Option<&str>,
        _body: &str,
    ) -> Option<(crate::hooks::WaitKind, Option<String>)> {
        None
    }

    /// Whether the agent can start a new session in the same process without
    /// a session start, so that a prompt under a new id is the first sign
    /// of it, as Cursor's /clear is. The run then moves to that session.
    fn prompt_may_start_session(&self) -> bool {
        false
    }

    /// Whether `process` is the agent's own: the one a hook or a command
    /// runs under, and the one Pinrail spawned for a run. By default, a
    /// process named after one of the agent's binaries. An agent that runs
    /// under an interpreter, or from a file of another name, says so.
    fn is_own_process(&self, process: &crate::process::Process) -> bool {
        self.spec().binaries.contains(&process.name.as_str())
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

/// How to type a message into the agent's terminal: the paste, the wait
/// for its output to settle, the keys that submit it, and the hook whose
/// report of the prompt confirms that the agent received it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubmitPlan {
    /// Whether the text goes in a bracketed paste.
    pub bracketed: bool,
    /// How long the output must be quiet after the paste.
    pub settle_ms: u64,
    /// The longest wait after the paste.
    pub max_ms: u64,
    /// The keys that submit the prompt.
    pub submit: &'static str,
    /// Keys typed before the paste that clear the agent's input, for an
    /// agent that leaves text there, such as Cursor after an interrupt;
    /// empty for the others.
    pub clear: &'static str,
    /// The hook event, as Pinrail names it, that reports the prompt.
    pub confirms: Option<&'static str>,
}

/// The plan of Claude Code and Codex: a bracketed paste, 150 ms for the
/// output to settle and 1 s at most, then Enter, confirmed by the prompt
/// hook.
pub(crate) const PASTE_AND_ENTER: SubmitPlan = SubmitPlan {
    bracketed: true,
    settle_ms: 150,
    max_ms: 1_000,
    submit: "\r",
    clear: "",
    confirms: Some("prompt"),
};

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
        "antigravity" => &Antigravity,
        "grok" => &Grok,
        "copilot" => &Copilot,
        "pi" => &Pi,
        "cursor" => &Cursor,
        "opencode" => &OpenCode,
        _ => return None,
    };
    Some(agent)
}

/// Whether `process` is the agent `id`'s own: as its adapter recognises it,
/// or, for an agent with none, by the names of its binaries.
pub fn runs_as(id: &str, process: &crate::process::Process) -> bool {
    match adapter(id) {
        Some(agent) => agent.is_own_process(process),
        None => crate::registry::find(id)
            .is_some_and(|spec| spec.binaries.contains(&process.name.as_str())),
    }
}

/// The registry entry of an adapter's agent.
pub(crate) fn spec(id: &str) -> &'static AgentSpec {
    crate::registry::find(id).expect("an adapter's agent is in the registry")
}
