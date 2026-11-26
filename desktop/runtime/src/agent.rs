//! What Pinrail does with an agent beyond the facts of its registry entry.
//! Each agent with behaviour of its own has an adapter in `agents/`; the
//! others are data only, and have none.

use std::path::Path;

use serde_json::Value;

use crate::agents::{Antigravity, ClaudeCode, Codex, Copilot, Cursor, Grok, OpenCode, Pi};
use crate::connection::{Shell, SkillFiles, SkillState};
use crate::hooks::HookReport;
use crate::integration::HookState;
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
        _shell: Shell,
    ) -> Result<usize, String> {
        Ok(0)
    }

    /// Where the agent's `pinrail` skill goes under `home`; `None` for an
    /// agent that takes no skill.
    fn skill_path(&self, home: &Path) -> Option<std::path::PathBuf> {
        Some(crate::connection::skill_path(
            home,
            self.spec().folders.as_ref()?,
        ))
    }

    /// Whether the agent is installed under `home`: its configuration
    /// folder is there.
    fn found(&self, home: &Path) -> bool {
        self.spec()
            .folders
            .as_ref()
            .is_some_and(|folders| home.join(folders.config).is_dir())
    }

    /// What is in the agent's place for the skill with `sha`. A connected
    /// skill counts as outdated while a file that goes with it is missing
    /// or another version's, so that updating it writes the file.
    fn skill_state(&self, home: &Path, env: Env, sha: &str) -> SkillState {
        let Some(path) = self.skill_path(home) else {
            return SkillState::Absent;
        };
        match crate::connection::state_of(&path, sha) {
            SkillState::Connected
                if !self
                    .skill_files(home, env)
                    .iter()
                    .all(|file| file.state() == HookState::Current) =>
            {
                SkillState::Outdated
            }
            state => state,
        }
    }

    /// What Pinrail's hooks, extension or plugin for the agent are in its
    /// file; `None` for an agent whose status Pinrail does not report.
    fn reporting_state(&self, home: &Path, env: Env) -> Option<HookState> {
        self.integration(home, env)
            .map(|plan| crate::integration::file_state(&plan))
    }

    /// Connects an installed agent, or brings its connection up to date:
    /// writes the skill and the files that go with it, then installs the
    /// hooks, extension or plugin that report its status, and trusts them
    /// where the agent asks for that. A `pinrail` skill of the person's own
    /// is never replaced, nor a hooks file Pinrail cannot read; the other
    /// part is still added. `shell` runs the agent's program for its trust
    /// step.
    fn connect(
        &self,
        home: &Path,
        env: Env,
        skill: &SkillFiles,
        shell: Shell,
    ) -> Result<(), String> {
        let label = self.spec().label;
        let path = self
            .skill_path(home)
            .ok_or_else(|| format!("Pinrail has no skill for {label}"))?;
        if !self.found(home) {
            let config = self.spec().folders.as_ref().map_or("", |f| f.config);
            return Err(format!("{label} is not installed: ~/{config} is not there"));
        }
        let theirs = crate::connection::state_of(&path, skill.sha) == SkillState::Theirs;
        if !theirs {
            // the files that go with it first, so the skill counts as
            // connected only once they are there
            for file in self.skill_files(home, env) {
                file.apply(true)?;
            }
            crate::connection::write_skill(&path, skill)?;
        }
        let plan = self
            .integration(home, env)
            .filter(|plan| crate::integration::file_state(plan) != HookState::Unreadable);
        let Some(plan) = plan else {
            return if theirs {
                Err(crate::connection::theirs(&path))
            } else {
                Ok(())
            };
        };
        crate::integration::apply(&plan, true)?;
        self.trust_hooks(&plan, shell).map(|_| ()).map_err(|e| {
            format!(
                "Pinrail installed its hooks, but could not trust them in {label}: {e}. \
                 {label} asks you to review them when it next starts."
            )
        })
    }

    /// Removes what connecting the agent added: the skill and the files
    /// that go with it, and the hooks, extension or plugin. A skill of the
    /// person's own stays, and so does a file of theirs at the path of one
    /// of Pinrail's, or a hooks file Pinrail cannot read.
    fn disconnect(&self, home: &Path, env: Env) -> Result<(), String> {
        if let Some(path) = self.skill_path(home)
            && fs_text(&path).is_none_or(|text| crate::connection::managed(&text))
        {
            crate::connection::remove_skill(&path)?;
            for file in self.skill_files(home, env) {
                if file.state() != HookState::Unreadable {
                    file.apply(false)?;
                }
            }
        }
        if let Some(plan) = self.integration(home, env)
            && !matches!(
                crate::integration::file_state(&plan),
                HookState::Absent | HookState::Unreadable
            )
        {
            crate::integration::apply(&plan, false)?;
        }
        Ok(())
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

    /// The latest turn the agent's own transcript at `transcript` holds,
    /// for the core to take what a hook's report would have given when the
    /// report was lost. `None` for an agent whose transcript Pinrail does not
    /// read, and for a transcript it cannot.
    fn transcript_turn(&self, _transcript: &Path) -> Option<TranscriptTurn> {
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

    /// The flags that turn the agent's own permission checks off, from its
    /// `--help`: a flag alone, or `flag=value` for a flag that does so
    /// only with that value. The person may give them in the agent's
    /// settings, which then warn about them; one start's arguments may not.
    fn skip_checks_flags(&self) -> &'static [&'static str] {
        &[]
    }

    /// An example of the person's arguments for the agent, from its
    /// `--help`, which the agent's settings show in the empty field.
    fn args_example(&self) -> Option<&'static str> {
        None
    }

    /// Flags Pinrail sets itself to name, resume or prompt the session,
    /// which a person's arguments may not give again.
    fn own_flags(&self) -> &'static [&'static str] {
        &[]
    }

    /// `resume`'s command with the person's arguments `extra`, after it by
    /// default; `None` for an agent Pinrail does not resume.
    fn resume_with(&self, session: &str, extra: &[String]) -> Option<Vec<String>> {
        let mut argv = self.resume(session)?;
        argv.extend(extra.iter().cloned());
        Some(argv)
    }

    /// The arguments that let the agent read the files in `folder`, outside
    /// the folder it works in, without asking, such as the files the person
    /// attaches to sessions. None for an agent that reads them already.
    fn read_folder_args(&self, _folder: &str) -> Vec<String> {
        Vec::new()
    }
}

/// A command with the arguments that let the agent read `folder`, right
/// after the program's name.
pub fn with_read_folder(agent: &dyn Agent, mut argv: Vec<String>, folder: &str) -> Vec<String> {
    let args = agent.read_folder_args(folder);
    if !args.is_empty() && !argv.is_empty() {
        argv.splice(1..1, args);
    }
    argv
}

/// The latest turn of an agent's transcript.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TranscriptTurn {
    /// The agent's own id for the turn, as its hooks report it.
    pub agent_turn: Option<String>,
    /// The person's prompt, when the transcript's part that was read holds it.
    pub prompt: Option<String>,
    /// When the person sent the prompt, as the transcript records it.
    pub prompted_at: Option<String>,
    /// The agent's last message of the turn, if it wrote one.
    pub reply: Option<String>,
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
    /// The first task, for an agent that takes none in its command:
    /// Pinrail types it as the first prompt once the agent is idle.
    pub type_task: Option<String>,
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

/// The first of `args` that turns the agent's permission checks off, as
/// written: one of the agent's own flags, alone or with `=value`, or a
/// flag that does so with a value, given as `flag=value` or as two
/// arguments.
pub fn skips_checks<'a>(agent: &dyn Agent, args: &'a [String]) -> Option<&'a str> {
    args.iter().enumerate().find_map(|(i, arg)| {
        let (flag, value) = match arg.split_once('=') {
            Some((flag, value)) => (flag, Some(value)),
            None => (arg.as_str(), args.get(i + 1).map(String::as_str)),
        };
        let skips = agent
            .skip_checks_flags()
            .iter()
            .any(|known| match known.split_once('=') {
                Some((known, only)) => flag == known && value == Some(only),
                None => flag == *known,
            });
        skips.then_some(arg.as_str())
    })
}

/// The arguments in `line`, split as a shell would: by spaces, with
/// quotes and backslashes keeping one argument whole.
pub fn split_args(line: &str) -> Result<Vec<String>, String> {
    shlex::split(line).ok_or_else(|| "has a quote that is not closed".to_string())
}

/// The first of `args` that gives again a flag Pinrail sets itself for the
/// agent, alone or with `=value`.
pub fn reserved_arg<'a>(agent: &dyn Agent, args: &'a [String]) -> Option<&'a str> {
    args.iter()
        .find(|arg| {
            let flag = arg.split_once('=').map_or(arg.as_str(), |(flag, _)| flag);
            agent.own_flags().contains(&flag)
        })
        .map(String::as_str)
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
    #[cfg(feature = "stand-in")]
    if name == crate::agents::STAND_IN.id {
        return Some(&crate::agents::StandIn);
    }
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

/// The agent an id or an alias names, to connect: its adapter, or for an
/// agent with none, one that does only what the trait does by default.
pub fn connector(name: &str) -> Option<&'static dyn Agent> {
    if let Some(agent) = adapter(name) {
        return Some(agent);
    }
    let id = crate::registry::find(name)?.id;
    let at = crate::registry::all().iter().position(|a| a.id == id)?;
    Some(&PLAIN[at])
}

/// An agent with no adapter, by its place in the registry.
struct Plain(usize);

static PLAIN: std::sync::LazyLock<Vec<Plain>> =
    std::sync::LazyLock::new(|| (0..crate::registry::all().len()).map(Plain).collect());

impl Agent for Plain {
    fn spec(&self) -> &'static AgentSpec {
        &crate::registry::all()[self.0]
    }

    fn parse_hook(&self, _event: &str, _input: &Value, _env: Env) -> Option<HookReport> {
        None
    }
}

/// The text of the file at `path`, if it can be read.
fn fs_text(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
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

/// What the adapters' tests of their flags share.
#[cfg(test)]
pub(crate) mod testing {
    use super::{Agent, skips_checks};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// `agent` finds that each line of `skipping` turns its checks off,
    /// through the argument given beside it as written, and that none of
    /// `checked` does.
    pub fn recognizes(agent: &dyn Agent, skipping: &[(&[&str], &str)], checked: &[&[&str]]) {
        let id = agent.spec().id;
        for (given, flag) in skipping {
            assert_eq!(
                skips_checks(agent, &args(given)),
                Some(*flag),
                "{id}: {given:?}"
            );
        }
        for given in checked {
            assert_eq!(skips_checks(agent, &args(given)), None, "{id}: {given:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The agents without an adapter, which take the skill alone: their
    /// configuration folders and where the skill goes.
    const PLAIN_AGENTS: &[(&str, &str, &str)] = &[
        ("goose", ".config/goose", ".config/goose/skills"),
        ("amp", ".config/amp", ".config/amp/skills"),
        ("kiro", ".kiro", ".kiro/skills"),
        ("augment", ".augment", ".augment/skills"),
        ("cline", ".cline", ".cline/skills"),
        ("hermes-agent", ".hermes", ".hermes/skills"),
    ];

    #[test]
    fn an_agent_without_an_adapter_is_connected_and_removed_in_its_folders() {
        for (id, config, skills) in PLAIN_AGENTS {
            assert!(
                adapter(id).is_none(),
                "{id} has an adapter, whose tests cover it"
            );
            let case = crate::connection::testing::Case {
                config,
                skills,
                reporting: None,
                theirs: None,
            };
            let shell = crate::connection::testing::no_shell;
            crate::connection::testing::connects_and_removes(connector(id).unwrap(), &case, &shell);
        }
    }

    #[test]
    fn every_adapter_s_example_is_a_line_it_takes_as_it_is() {
        for spec in crate::registry::all() {
            let Some(agent) = adapter(spec.id) else {
                continue;
            };
            let example = agent
                .args_example()
                .unwrap_or_else(|| panic!("{}: no example", spec.id));
            let args = split_args(example).unwrap();
            assert!(!args.is_empty(), "{}", spec.id);
            assert_eq!(skips_checks(agent, &args), None, "{}", spec.id);
            assert_eq!(reserved_arg(agent, &args), None, "{}", spec.id);
        }
    }

    #[test]
    fn every_agent_that_takes_the_skill_has_a_connector_and_a_test() {
        for spec in crate::registry::all()
            .iter()
            .filter(|a| a.folders.is_some())
        {
            assert!(connector(spec.id).is_some(), "{}", spec.id);
            assert!(
                adapter(spec.id).is_some() || PLAIN_AGENTS.iter().any(|(id, ..)| *id == spec.id),
                "{} has no adapter and is not in PLAIN_AGENTS",
                spec.id
            );
        }
    }

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn claude_code_may_read_the_folder_by_a_rule_after_the_program_name() {
        let claude = adapter("claude-code").unwrap();
        let rule = r#"{"permissions":{"allow":["Read(//data/sessions/**)"]}}"#;
        assert_eq!(
            with_read_folder(
                claude,
                argv(&["claude", "--resume", "s-1"]),
                "/data/sessions"
            ),
            argv(&["claude", "--settings", rule, "--resume", "s-1"])
        );
    }

    #[test]
    fn an_agent_that_reads_anywhere_gets_no_flag() {
        let codex = adapter("codex").unwrap();
        let command = argv(&["codex", "resume", "s-1"]);
        assert_eq!(
            with_read_folder(codex, command.clone(), "/data/sessions"),
            command
        );
    }
}
