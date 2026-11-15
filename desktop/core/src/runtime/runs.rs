//! The agents Pinrail starts in a terminal of the host. A run is written as
//! `starting` before the host is asked to spawn it, with its id in the
//! terminal's `run` label, so a spawn whose reply was lost is found again
//! when the core reconnects and is never started twice. Each run carries a
//! token in its environment, of which the core keeps only the digest.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Utc};
use pinrail_host::link::{HostError, HostLink, HostState};
use serde_json::{Value, json};

use crate::db::{NewRun, RunFilters, RunRow};
use crate::error::Error;
use crate::events::Notice;

/// The host every run is on, until Pinrail reaches other computers.
pub const LOCAL: &str = "local";

/// How long a stopped agent has to exit before it is killed.
const STOP_GRACE_MS: u64 = 2_000;

/// The environment variable that carries a run's token.
pub const TOKEN_VARIABLE: &str = "PINRAIL_RUN_TOKEN";

/// What to start.
#[derive(Debug, Clone)]
pub struct Start {
    pub agent: String,
    pub folder: String,
    /// The first task, typed in as the agent's first prompt.
    pub task: Option<String>,
    /// Extra arguments for the agent.
    pub args: Vec<String>,
    pub cols: u16,
    pub rows: u16,
    /// `app` or `cli`.
    pub created_by: String,
}

/// What to resume: a stopped session, in a new terminal of the size
/// given.
#[derive(Debug, Clone)]
pub struct Resume {
    pub agent: String,
    pub session: String,
    pub cols: u16,
    pub rows: u16,
    /// `app` or `cli`.
    pub created_by: String,
    /// Resume a session still running in another terminal, for an agent
    /// that allows it: the person was warned.
    pub anyway: bool,
}

/// How soon after it began a resumed agent that exits with an error is
/// taken to have failed to resume.
const RESUME_FAILS_WITHIN: chrono::Duration = chrono::Duration::seconds(3);

/// Why a run could not be started or stopped.
#[derive(Debug)]
pub enum RunError {
    /// A field of the request is wrong, as the message says.
    Invalid {
        field: String,
        message: String,
    },
    /// The agent is not installed on the host.
    AgentNotFound(String),
    /// The host runs as many terminals as it may.
    Limit(String),
    /// The folder does not exist.
    FolderMissing(String),
    /// The terminal host is not connected.
    HostOffline,
    /// No such run.
    NotFound(String),
    /// The host failed to start the agent, as the message says.
    StartFailed(String),
    /// The session already runs: in a run of Pinrail's, or in a terminal
    /// outside Pinrail, by the process that last reported, which can be
    /// resumed `anyway` when its agent allows two processes on a session.
    AlreadyRunning {
        run: Option<Box<RunRow>>,
        process: Option<i64>,
        anyway: bool,
    },
    /// Pinrail cannot resume the session, as the message says.
    NotResumable(String),
    /// The host did not type the message: `used_since_idle`,
    /// `not_foreground`, `exited`, `busy`, or `not_typed` for an agent
    /// Pinrail does not type into.
    NotTyped(&'static str),
    Core(Error),
}

impl RunError {
    fn invalid(field: &str, message: impl Into<String>) -> Self {
        RunError::Invalid {
            field: field.to_string(),
            message: message.into(),
        }
    }

    /// The error's kind, as the API names it.
    pub fn code(&self) -> &'static str {
        match self {
            RunError::Invalid { .. } => "invalid",
            RunError::AgentNotFound(_) => "agent_not_found",
            RunError::Limit(_) => "limit",
            RunError::FolderMissing(_) => "folder_missing",
            RunError::HostOffline => "host_offline",
            RunError::NotFound(_) => "not_found",
            RunError::StartFailed(_) => "start_failed",
            RunError::AlreadyRunning { .. } => "already_running",
            RunError::NotResumable(_) => "not_resumable",
            RunError::NotTyped(code) => code,
            RunError::Core(_) => "internal",
        }
    }
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Invalid { field, message } => write!(f, "{field}: {message}"),
            RunError::AgentNotFound(message)
            | RunError::Limit(message)
            | RunError::StartFailed(message)
            | RunError::NotResumable(message) => write!(f, "{message}"),
            RunError::NotTyped(code) => write!(
                f,
                "{}",
                match *code {
                    "used_since_idle" => "You used the terminal after the agent finished.",
                    "not_foreground" => "Another program holds the agent's terminal.",
                    "exited" => "The agent has exited.",
                    "busy" => "Another message is being typed.",
                    _ => "Pinrail does not type messages into this agent.",
                }
            ),
            RunError::AlreadyRunning { run: Some(_), .. } => {
                write!(f, "This session is already running.")
            }
            RunError::AlreadyRunning { run: None, .. } => {
                write!(f, "This session is already running in another terminal.")
            }
            RunError::FolderMissing(folder) => write!(f, "{folder} does not exist"),
            RunError::HostOffline => write!(f, "the terminal host is not connected"),
            RunError::NotFound(run) => write!(f, "run {run} not found"),
            RunError::Core(error) => write!(f, "{}", error.message()),
        }
    }
}

impl std::error::Error for RunError {}

impl From<rusqlite::Error> for RunError {
    fn from(error: rusqlite::Error) -> Self {
        RunError::Core(error.into())
    }
}

/// The spawns the host has not answered yet. Held while a spawn's answer
/// is recorded and while an exit is matched to its run, so that an agent
/// that exits at once still ends its run.
#[derive(Debug, Default)]
pub(super) struct Spawns {
    /// The runs being spawned, which reconciling leaves alone.
    running: HashSet<String>,
    /// The `exited` events of terminals no run had yet, by terminal.
    exits: HashMap<String, Value>,
    /// The runs started with a `local_id` in the last 10 minutes, by it:
    /// the run, and when it started.
    recent: HashMap<String, (String, std::time::Instant)>,
    /// The resumes being spawned, by session as `(agent, id)`: a second
    /// resume of the session meanwhile returns the same run.
    resuming: HashMap<(String, String), String>,
}

/// How long a start's `local_id` names its run.
const LOCAL_ID_FOR: std::time::Duration = std::time::Duration::from_secs(600);

/// What reconciling with the host's terminals changed.
#[derive(Debug, Default)]
pub struct Reconciled {
    /// The runs that ended, with why.
    pub ended: Vec<RunRow>,
    /// The terminals whose `run` label names no live run, to stop.
    pub orphans: Vec<String>,
}

impl super::Runtime {
    /// Starts `start.agent` in a new terminal of the host, and returns its
    /// run: `running`, or still `starting` when the host's reply was lost,
    /// which the next reconnect settles.
    pub async fn start(&self, start: &Start) -> Result<RunRow, RunError> {
        let agent = pinrail_runtime::adapter(&start.agent).ok_or_else(|| {
            RunError::invalid("agent", format!("Pinrail cannot start {}", start.agent))
        })?;
        if let Some(arg) = pinrail_runtime::denied_arg(agent, &start.args) {
            return Err(RunError::invalid(
                "args",
                format!("Pinrail does not start agents with permission checks turned off ({arg})."),
            ));
        }
        if start.folder.trim().is_empty() {
            return Err(RunError::invalid("folder", "name the folder to start in"));
        }
        let link = self.ready_host().ok_or(RunError::HostOffline)?;
        let session_id = new_uuid();
        let plan = agent
            .start(&pinrail_runtime::StartRequest {
                task: start.task.as_deref(),
                extra_args: &start.args,
                session_id: &session_id,
            })
            .ok_or_else(|| {
                RunError::invalid("agent", format!("Pinrail cannot start {}", start.agent))
            })?;

        let id = crate::id::next_run();
        let token = new_token();
        let run = self.db.insert_run(
            &NewRun {
                id: &id,
                host: LOCAL,
                agent: &start.agent,
                kind: "start",
                folder: &start.folder,
                argv: &plan.argv,
                task: start.task.as_deref(),
                args: &start.args,
                created_by: &start.created_by,
                token_digest: &super::digest(&token),
                session: plan.session.as_deref(),
            },
            Utc::now(),
        )?;
        self.spawn(&link, run, &plan.argv, start.cols, start.rows, token)
            .await
    }

    /// Resumes a stopped session in a new terminal, in the folder it
    /// worked in, and returns its run. Refuses a session that already
    /// runs, in Pinrail or in another terminal, and one whose folder is
    /// gone. A second resume while the first is starting returns the
    /// first's run.
    pub async fn resume(&self, resume: &Resume) -> Result<RunRow, RunError> {
        let key = (resume.agent.clone(), resume.session.clone());
        let now = Utc::now();
        let session = self
            .db
            .session(&resume.agent, &resume.session, now)?
            .ok_or_else(|| {
                RunError::NotFound(format!("session {}/{}", resume.agent, resume.session))
            })?;
        let agent = pinrail_runtime::adapter(&resume.agent).ok_or_else(|| {
            RunError::NotResumable(format!("Pinrail cannot resume {} sessions.", resume.agent))
        })?;
        let argv = agent.resume(&resume.session).ok_or_else(|| {
            RunError::NotResumable(format!("Pinrail cannot resume {} sessions.", resume.agent))
        })?;
        let link = self.ready_host().ok_or(RunError::HostOffline)?;
        let token = new_token();
        let run = {
            // checked and recorded under one lock, so that two resumes of
            // the session never both start
            let mut spawns = self.spawns();
            if let Some(run) = spawns
                .resuming
                .get(&key)
                .and_then(|id| self.db.run(id).ok().flatten())
            {
                return Ok(run);
            }
            if let Some(run) = self
                .live_runs()?
                .into_iter()
                .find(|r| r.session.as_ref() == Some(&key))
            {
                return Err(RunError::AlreadyRunning {
                    run: Some(Box::new(run)),
                    process: None,
                    anyway: false,
                });
            }
            let outside = self
                .db
                .session_process(&resume.agent, &resume.session)?
                .filter(|(pid, started)| crate::presence::alive(*pid, started));
            if let Some((pid, _)) = outside
                && !(resume.anyway && agent.resumes_while_running())
            {
                return Err(RunError::AlreadyRunning {
                    run: None,
                    process: Some(pid),
                    anyway: agent.resumes_while_running(),
                });
            }
            let folder = match session.folder.filter(|f| !f.is_empty()) {
                Some(folder) => folder,
                None => self
                    .db
                    .runs(&RunFilters {
                        session: Some(key.clone()),
                        ..RunFilters::default()
                    })?
                    .into_iter()
                    .next()
                    .map(|r| r.folder)
                    .ok_or_else(|| {
                        RunError::NotResumable("Pinrail does not know the session's folder.".into())
                    })?,
            };
            if !std::path::Path::new(&folder).is_dir() {
                return Err(RunError::FolderMissing(folder));
            }
            let id = crate::id::next_run();
            let run = self.db.insert_run(
                &NewRun {
                    id: &id,
                    host: LOCAL,
                    agent: &resume.agent,
                    kind: "resume",
                    folder: &folder,
                    argv: &argv,
                    task: None,
                    args: &[],
                    created_by: &resume.created_by,
                    token_digest: &super::digest(&token),
                    session: Some(&resume.session),
                },
                now,
            )?;
            spawns.resuming.insert(key.clone(), id);
            run
        };
        let result = self
            .spawn(&link, run, &argv, resume.cols, resume.rows, token)
            .await;
        self.spawns().resuming.remove(&key);
        result
    }

    /// Types `text` into a live run's terminal as a prompt, with its
    /// agent's plan, and submits it. The host types only while the run's
    /// own agent process holds the terminal, and, unless `anyway`, only if
    /// the person has not used the terminal since `idle_since`, when the
    /// agent last became idle. Answers once the text has been submitted.
    pub async fn type_message(
        &self,
        run: &str,
        text: &str,
        idle_since: Option<DateTime<Utc>>,
        anyway: bool,
    ) -> Result<(), RunError> {
        let row = self
            .db
            .run(run)?
            .ok_or_else(|| RunError::NotFound(run.to_string()))?;
        let (Some(terminal), true) = (row.terminal.as_deref(), row.is_live()) else {
            return Err(RunError::NotTyped("exited"));
        };
        let plan = pinrail_runtime::adapter(&row.agent)
            .and_then(|a| a.submit())
            .ok_or(RunError::NotTyped("not_typed"))?;
        let link = self.ready_host().ok_or(RunError::HostOffline)?;
        let reply = link
            .call(
                "terminal.submit",
                json!({
                    "terminal": terminal,
                    "text": text,
                    "bracketed": plan.bracketed,
                    "settle_ms": plan.settle_ms,
                    "max_ms": plan.max_ms,
                    "submit": plan.submit,
                    "expect_pid": row.process.as_ref().map(|(pid, _)| pid),
                    "idle_since": idle_since.map(crate::reviews::iso),
                    "anyway": anyway,
                }),
            )
            .await;
        match reply {
            Ok(_) => Ok(()),
            Err(HostError::Refused { code, message }) => Err(match code.as_str() {
                "used_since_idle" => RunError::NotTyped("used_since_idle"),
                "not_foreground" => RunError::NotTyped("not_foreground"),
                "busy" => RunError::NotTyped("busy"),
                "exited" | "not_found" => RunError::NotTyped("exited"),
                _ => RunError::StartFailed(message),
            }),
            Err(_) => Err(RunError::HostOffline),
        }
    }

    /// Asks the host to spawn a run just recorded as `starting`, with the
    /// token in its environment, and returns the run as the reply leaves
    /// it.
    async fn spawn(
        &self,
        link: &HostLink,
        run: RunRow,
        argv: &[String],
        cols: u16,
        rows: u16,
        token: String,
    ) -> Result<RunRow, RunError> {
        let id = run.id.clone();
        let folder = run.folder.clone();
        let mut env: BTreeMap<String, String> = self.run_env.clone();
        env.insert(TOKEN_VARIABLE.to_string(), token);
        self.spawns().running.insert(id.clone());
        let reply = link
            .call(
                "terminal.spawn",
                json!({
                    "argv": argv,
                    "cwd": folder,
                    "cols": cols,
                    "rows": rows,
                    "env": env,
                    "labels": {"run": id},
                }),
            )
            .await;
        let mut spawns = self.spawns();
        spawns.running.remove(&id);
        match reply {
            Ok(reply) => {
                let terminal = reply["terminal"].as_str().unwrap_or_default();
                let run = self
                    .spawned(&id, terminal, process_of(&reply))?
                    .unwrap_or(run);
                // an agent that exited before its spawn was recorded
                match spawns.exits.remove(terminal) {
                    Some(exit) => {
                        drop(spawns);
                        Ok(self.exited(&exit)?.unwrap_or(run))
                    }
                    None => Ok(run),
                }
            }
            // whether the host spawned it is known on the next connect
            Err(HostError::Unknown) => Ok(run),
            Err(error) => {
                drop(spawns);
                self.end(&id, "start_failed", None, None)?;
                Err(match error {
                    HostError::Refused { code, message } => match code.as_str() {
                        "not_found" => {
                            // what the host found of the agents is out of date
                            self.lock_startable().stale = true;
                            RunError::AgentNotFound(message)
                        }
                        "folder_missing" => RunError::FolderMissing(folder.clone()),
                        "limit" => RunError::Limit(message),
                        _ => RunError::StartFailed(message),
                    },
                    _ => RunError::HostOffline,
                })
            }
        }
    }

    /// [`Self::start`], once for each `local_id` in 10 minutes: a start
    /// with the `local_id` of an earlier one returns that run.
    pub async fn start_once(
        &self,
        local_id: Option<&str>,
        start: &Start,
    ) -> Result<RunRow, RunError> {
        let Some(local_id) = local_id else {
            return self.start(start).await;
        };
        let earlier = {
            let mut spawns = self.spawns();
            spawns
                .recent
                .retain(|_, (_, at)| at.elapsed() < LOCAL_ID_FOR);
            spawns.recent.get(local_id).map(|(run, _)| run.clone())
        };
        if let Some(run) = earlier.and_then(|id| self.db.run(&id).ok().flatten()) {
            return Ok(run);
        }
        let run = self.start(start).await?;
        self.spawns().recent.insert(
            local_id.to_string(),
            (run.id.clone(), std::time::Instant::now()),
        );
        Ok(run)
    }

    /// Stops a run's agent: it has 2 seconds to exit, or none with
    /// `force`. The run ends when the host reports that the agent exited.
    pub async fn stop(&self, run: &str, force: bool) -> Result<(), RunError> {
        let row = self
            .db
            .run(run)?
            .ok_or_else(|| RunError::NotFound(run.to_string()))?;
        if !row.is_live() {
            return Ok(());
        }
        let Some(terminal) = row.terminal else {
            // not spawned yet: a terminal that turns up for it later names
            // no live run, and is stopped when the core reconciles
            self.end(run, "stopped", None, None)?;
            return Ok(());
        };
        let link = self.ready_host().ok_or(RunError::HostOffline)?;
        let grace = if force { 0 } else { STOP_GRACE_MS };
        match link
            .call(
                "terminal.stop",
                json!({"terminal": terminal, "grace_ms": grace}),
            )
            .await
        {
            Ok(_) => Ok(()),
            // the terminal is gone: the next reconcile ends the run
            Err(HostError::Refused { code, .. }) if code == "not_found" => Ok(()),
            Err(HostError::Refused { message, .. }) => Err(RunError::StartFailed(message)),
            Err(_) => Err(RunError::HostOffline),
        }
    }

    /// Ends the run of a terminal that exited, for the host's `exited`
    /// event. Returns the run when it ended now.
    pub fn exited(&self, params: &Value) -> Result<Option<RunRow>, RunError> {
        let Some(terminal) = params["terminal"].as_str() else {
            return Ok(None);
        };
        let mut spawns = self.spawns();
        let Some(run) = self
            .live_runs()?
            .into_iter()
            .find(|r| r.terminal.as_deref() == Some(terminal))
        else {
            // the exit of a terminal whose spawn has not answered yet is
            // kept for the run it turns out to be
            if !spawns.running.is_empty() {
                spawns.exits.insert(terminal.to_string(), params.clone());
            }
            return Ok(None);
        };
        drop(spawns);
        let reason = if params["reason"] == "stopped" {
            "stopped"
        } else {
            exit_reason(&run, params["code"].as_i64(), params["signal"].as_str())
        };
        self.end(
            &run.id,
            reason,
            params["code"].as_i64(),
            params["signal"].as_str(),
        )
    }

    /// Brings the runs in line with the terminals the host lists on
    /// connecting: a starting run the host spawned is running, a run whose
    /// terminal exited or is gone ends, and a terminal whose `run` label
    /// names no live run is to be stopped. A run being spawned right now is
    /// left alone.
    pub fn reconcile(&self, terminals: &Value) -> Result<Reconciled, RunError> {
        let terminals = terminals.as_array().cloned().unwrap_or_default();
        let by_run: HashMap<&str, &Value> = terminals
            .iter()
            .filter_map(|t| Some((t["labels"]["run"].as_str()?, t)))
            .collect();
        let by_id: HashMap<&str, &Value> = terminals
            .iter()
            .filter_map(|t| Some((t["terminal"].as_str()?, t)))
            .collect();
        let spawning = self.spawns().running.clone();
        // a host in a new boot of the computer: its agents ended with it
        let boot = pinrail_host::boot_id();
        let restarted = match (&boot, self.db.host_boot(LOCAL)?) {
            (Some(now), Some(before)) => *now != before,
            _ => false,
        };
        let mut done = Reconciled::default();
        let mut live: HashSet<String> = HashSet::new();
        for run in self.live_runs()? {
            if spawning.contains(&run.id) {
                live.insert(run.id);
                continue;
            }
            let found = match &run.terminal {
                Some(terminal) => by_id.get(terminal.as_str()),
                None => by_run.get(run.id.as_str()),
            };
            let Some(found) = found else {
                let reason = if run.terminal.is_some() {
                    if restarted { "restart" } else { "host_lost" }
                } else {
                    // the spawn never reached the host
                    "start_failed"
                };
                done.ended.extend(self.end(&run.id, reason, None, None)?);
                continue;
            };
            if run.terminal.is_none() {
                let terminal = found["terminal"].as_str().unwrap_or_default();
                self.spawned(&run.id, terminal, process_of(found))?;
            }
            if let Some(exit) = found.get("exit") {
                done.ended.extend(self.end(
                    &run.id,
                    exit_reason(&run, exit["code"].as_i64(), exit["signal"].as_str()),
                    exit["code"].as_i64(),
                    exit["signal"].as_str(),
                )?);
            } else {
                live.insert(run.id);
            }
        }
        done.orphans = terminals
            .iter()
            .filter(|t| t.get("exit").is_none())
            .filter(|t| {
                t["labels"]["run"]
                    .as_str()
                    .is_some_and(|run| !live.contains(run))
            })
            .filter_map(|t| t["terminal"].as_str().map(str::to_string))
            .collect();
        if let Some(boot) = boot {
            self.db.set_host_boot(LOCAL, &boot)?;
        }
        Ok(done)
    }

    /// The runs a restart of the computer ended, whose sessions were not
    /// resumed since, unless the person dismissed them, each with its
    /// session's title.
    pub fn restarted(&self) -> Result<Vec<(RunRow, Option<String>)>, RunError> {
        let now = Utc::now();
        Ok(self
            .db
            .restarted_runs(LOCAL)?
            .into_iter()
            .map(|run| {
                let title = run
                    .session
                    .as_ref()
                    .and_then(|(agent, id)| self.db.session(agent, id, now).ok().flatten()?.title);
                (run, title)
            })
            .collect())
    }

    /// Hides the runs [`Self::restarted`] lists until the next restart.
    pub fn dismiss_restart(&self) -> Result<(), RunError> {
        Ok(self.db.dismiss_restart(LOCAL, Utc::now())?)
    }

    /// Forgets the tokens of runs that ended more than 5 minutes ago.
    pub fn forget_tokens(&self) -> Result<usize, RunError> {
        Ok(self.db.forget_run_tokens(Utc::now())?)
    }

    /// The last `lines` lines a run's terminal showed, from the host,
    /// which keeps an ended terminal for a day.
    pub async fn output(&self, id: &str, lines: usize) -> Result<String, RunError> {
        let run = self
            .db
            .run(id)?
            .ok_or_else(|| RunError::NotFound(format!("run {id}")))?;
        let terminal = run
            .terminal
            .ok_or_else(|| RunError::NotFound(format!("the terminal of run {id}")))?;
        let link = self.ready_host().ok_or(RunError::HostOffline)?;
        let reply = match link
            .call(
                "terminal.text",
                json!({ "terminal": terminal, "lines": lines }),
            )
            .await
        {
            Ok(reply) => reply,
            Err(HostError::Refused { message, .. }) => return Err(RunError::NotFound(message)),
            Err(_) => return Err(RunError::HostOffline),
        };
        Ok(last_lines(reply["text"].as_str().unwrap_or(""), lines))
    }

    /// A run's screen as a paint any emulator reads, with its size: the
    /// screen it ended with, once it has, while the host keeps it.
    pub async fn screen(&self, id: &str) -> Result<Value, RunError> {
        let run = self
            .db
            .run(id)?
            .ok_or_else(|| RunError::NotFound(format!("run {id}")))?;
        let terminal = run
            .terminal
            .ok_or_else(|| RunError::NotFound(format!("the terminal of run {id}")))?;
        let link = self.ready_host().ok_or(RunError::HostOffline)?;
        match link
            .call("terminal.screen", json!({ "terminal": terminal }))
            .await
        {
            Ok(reply) => Ok(json!({
                "paint": reply["paint"],
                "cols": reply["cols"],
                "rows": reply["rows"],
            })),
            Err(HostError::Refused { message, .. }) => Err(RunError::NotFound(message)),
            Err(_) => Err(RunError::HostOffline),
        }
    }

    /// The agents Pinrail can start as the host last found them, with
    /// when it looked, and whether it should look again: on `refresh`,
    /// after a start found an agent missing, or after an hour. Only the
    /// first request, with nothing found yet, waits for the host.
    pub async fn startable(&self, refresh: bool) -> Result<(Value, bool), RunError> {
        if self.lock_startable().found.is_none() {
            self.check_startable(None).await?;
        }
        let (found, checked_at, stale, begun) = {
            let cache = self.lock_startable();
            let (found, at) = cache.found.clone().ok_or(RunError::HostOffline)?;
            (found, at, cache.stale, cache.begun.unwrap_or(at))
        };
        // a check that began since covers what made it due
        let due = refresh || stale || Utc::now() - begun > STARTABLE_FOR;
        let last = self.db.latest_run_agent()?;
        let agents = startable_list(&startable_agents(), &found, last.as_deref());
        Ok((
            json!({ "agents": agents, "checked_at": crate::reviews::iso(checked_at) }),
            due,
        ))
    }

    /// Asks the host which agents' programs it finds, and keeps the
    /// answer with when the check began. Checks run one at a time; one
    /// asked for at `since` is skipped when another began after it, and
    /// one without `since` when there is any answer.
    pub async fn check_startable(&self, since: Option<DateTime<Utc>>) -> Result<(), RunError> {
        let _one = self.checking_startable.lock().await;
        // a check asked for from now on waits for one that begins after it
        self.startable_waiting
            .store(false, std::sync::atomic::Ordering::SeqCst);
        // with no `since`, any answer will do: one that came while this
        // waited for the check before it
        let done = match (since, &self.lock_startable().found) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(since), Some((_, at))) => *at >= since,
        };
        if done {
            return Ok(());
        }
        let began = Utc::now();
        {
            let mut cache = self.lock_startable();
            cache.stale = false;
            cache.begun = Some(began);
        }
        let found = self.find_startable().await;
        let mut cache = self.lock_startable();
        match found {
            Ok(found) => {
                cache.found = Some((found, began));
                Ok(())
            }
            Err(error) => {
                // the next request tries again
                cache.stale = true;
                Err(error)
            }
        }
    }

    /// What the host finds of each agent's program now.
    async fn find_startable(&self) -> Result<Vec<Value>, RunError> {
        let agents = startable_agents();
        let link = self.ready_host().ok_or(RunError::HostOffline)?;
        let names: Vec<&str> = agents.iter().map(|(_, program)| program.as_str()).collect();
        let mut params = json!({ "names": names });
        if let Some(path) = self.run_env.get("PATH") {
            params["path"] = json!(path);
        }
        match link.call("programs.find", params).await {
            Ok(reply) => Ok(reply["programs"].as_array().cloned().unwrap_or_default()),
            Err(HostError::Refused { message, .. }) => Err(RunError::StartFailed(message)),
            Err(_) => Err(RunError::HostOffline),
        }
    }

    fn lock_startable(&self) -> std::sync::MutexGuard<'_, Startable> {
        self.startable.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The link, when the host is connected.
    fn ready_host(&self) -> Option<HostLink> {
        self.host()
            .filter(|link| matches!(link.state(), HostState::Ready { .. }))
    }

    fn live_runs(&self) -> Result<Vec<RunRow>, RunError> {
        Ok(self.db.runs(&RunFilters {
            live: Some(true),
            host: Some(LOCAL.to_string()),
            ..RunFilters::default()
        })?)
    }

    fn spawns(&self) -> std::sync::MutexGuard<'_, Spawns> {
        self.spawns.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn spawned(
        &self,
        run: &str,
        terminal: &str,
        process: Option<(i64, String)>,
    ) -> Result<Option<RunRow>, RunError> {
        Ok(self
            .db
            .run_spawned(run, terminal, process)?
            .map(|(row, (event, attrs))| {
                self.bus
                    .publish(Notice::new(event, crate::events::RUN_STARTED, &attrs));
                row
            }))
    }

    fn end(
        &self,
        run: &str,
        reason: &str,
        code: Option<i64>,
        signal: Option<&str>,
    ) -> Result<Option<RunRow>, RunError> {
        Ok(self
            .db
            .end_run(run, reason, code, signal, Utc::now())?
            .map(|(row, (event, attrs))| {
                self.bus
                    .publish(Notice::new(event, crate::events::RUN_ENDED, &attrs));
                row
            }))
    }
}

/// Why a run whose agent exited on its own ended: `resume_failed` for a
/// resumed agent that exited with an error within 3 seconds, as when its
/// session can no longer be resumed, and `exited` otherwise.
fn exit_reason(run: &RunRow, code: Option<i64>, signal: Option<&str>) -> &'static str {
    let failed = code.is_some_and(|c| c != 0) || signal.is_some();
    let soon = crate::reviews::parse_datetime(&run.started_at)
        .is_some_and(|at| Utc::now() - at < RESUME_FAILS_WITHIN);
    if run.kind == "resume" && failed && soon {
        "resume_failed"
    } else {
        "exited"
    }
}

/// The agent process a spawn reply or a listed terminal names: its pid and
/// its start time.
fn process_of(terminal: &Value) -> Option<(i64, String)> {
    Some((
        terminal["pid"].as_i64()?,
        terminal["process_started"].as_str()?.to_string(),
    ))
}

/// 32 random bytes, in hex.
fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the operating system's random numbers");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A random UUID, version 4, for a session whose id Pinrail chooses.
fn new_uuid() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the operating system's random numbers");
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// The last `n` lines of `text` that are not blank at its end.
fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.trim_end().lines().map(str::trim_end).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// How long what the host found of the agents counts as current.
const STARTABLE_FOR: chrono::TimeDelta = chrono::TimeDelta::hours(1);

/// What the host last found of the agents' programs.
#[derive(Debug, Default)]
pub(super) struct Startable {
    /// Its answer, in the order of [`startable_agents`], and when the
    /// check that gave it began.
    found: Option<(Vec<Value>, DateTime<Utc>)>,
    /// A start found an agent missing since, so the answer is out of date.
    stale: bool,
    /// When the latest check began, which may still run.
    begun: Option<DateTime<Utc>>,
}

/// Every agent Pinrail can start, in Settings' order, with its program.
fn startable_agents() -> Vec<(&'static pinrail_runtime::AgentSpec, String)> {
    pinrail_runtime::all()
        .iter()
        .filter_map(|spec| {
            let plan =
                pinrail_runtime::adapter(spec.id)?.start(&pinrail_runtime::StartRequest {
                    task: None,
                    extra_args: &[],
                    session_id: "00000000-0000-4000-8000-000000000000",
                })?;
            Some((spec, plan.argv.first()?.clone()))
        })
        .collect()
}

/// The agents Pinrail can start, from what the host found of their
/// programs.
fn startable_list(
    agents: &[(&pinrail_runtime::AgentSpec, String)],
    found: &[Value],
    last: Option<&str>,
) -> Vec<Value> {
    agents
        .iter()
        .enumerate()
        .map(|(i, (spec, program))| {
            let one = found.get(i).unwrap_or(&Value::Null);
            json!({
                "id": spec.id,
                "name": spec.label,
                "program": program,
                "installed": one["path"].is_string(),
                "version": one["version"],
                "last_used": last == Some(spec.id),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agents() -> Vec<(&'static pinrail_runtime::AgentSpec, String)> {
        ["claude-code", "codex"]
            .iter()
            .map(|id| (pinrail_runtime::find(id).unwrap(), id.to_string()))
            .collect()
    }

    #[test]
    fn the_output_is_its_last_lines_without_the_blank_end() {
        let text = "one\ntwo  \nthree\n\n   \n";
        assert_eq!(last_lines(text, 2), "two\nthree");
        assert_eq!(last_lines(text, 20), "one\ntwo\nthree");
        assert_eq!(last_lines("", 20), "");
    }

    #[test]
    fn the_list_says_what_the_host_found() {
        let found = [
            json!({"name": "claude", "path": "/bin/claude", "version": "2.1.325"}),
            json!({"name": "codex", "path": null, "version": null}),
        ];
        let list = startable_list(&agents(), &found, Some("codex"));
        assert_eq!(list[0]["installed"], true);
        assert_eq!(list[0]["version"], "2.1.325");
        assert_eq!(list[1]["installed"], false);
        assert_eq!(list[1]["last_used"], true);
    }
}
