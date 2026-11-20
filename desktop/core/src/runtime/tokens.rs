//! Run tokens on reports. A run's token is in its agent's environment, so
//! every process the agent starts inherits it, a nested agent included.
//! The core therefore takes the token as the run's only when the report
//! comes from the run's own agent process, and ignores it otherwise.

use chrono::Utc;
use pinrail_runtime::process::Process;
use sha2::{Digest, Sha256};

use crate::db::{Report, ReportEvent, RunResolved, RunRow};
use crate::events::Notice;

/// The SHA-256 digest of a run token, in hex, as the core keeps it.
pub fn digest(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

/// The session a report is for, by Pinrail's id.
#[derive(Debug, Default, PartialEq)]
pub struct Admitted {
    pub session: String,
    /// The session the run left for the report's, on `/clear` or a fork,
    /// whose agent no longer works in it, as `(agent, id)`.
    pub cleared: Option<(String, String)>,
}

/// A report from a run's own process about a session that is not the
/// run's, as the message says.
#[derive(Debug, PartialEq)]
pub struct Mismatch(pub String);

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl super::Runtime {
    /// The session a report names by the agent and the agent's own id for
    /// it, `agent_session`, by Pinrail's id. Sent with the token of a run,
    /// from the run's own process, the report is for the run's session,
    /// which learns the agent's id from it if it did not know it; a session
    /// start for another session moves the run to that one; any other
    /// report for another session is a mismatch. Without a token, or with
    /// one that is unknown or comes from any other process, it is for the
    /// session with that id, created, hidden, if it is new.
    pub fn admit_report(
        &self,
        token: Option<&str>,
        agent: &str,
        agent_session: &str,
        report: &Report,
    ) -> Result<Result<Admitted, Mismatch>, crate::error::Error> {
        let now = Utc::now();
        let run = match token {
            Some(token) => self.db.run_for_token(&digest(token), now)?,
            None => None,
        };
        let lookup = |pid: u32| pinrail_runtime::process::process(pid);
        let Some(run) = run.filter(|run| {
            let agent = |p: &Process| pinrail_runtime::runs_as(&run.agent, p);
            own_process(run, report.process.as_ref(), &agent, &lookup)
        }) else {
            let (session, _) = self.db.resolve_session(agent, agent_session, now)?;
            return Ok(Ok(Admitted {
                session,
                cleared: None,
            }));
        };
        let source = match &report.event {
            ReportEvent::Started { source } => Some(source.as_deref()),
            _ => None,
        };
        let resolved = self.db.resolve_run_session(
            &run.id,
            agent,
            agent_session,
            source.is_some(),
            &report.origin,
            now,
        )?;
        let (session, bound) = match resolved {
            RunResolved::Session { id, bound, .. } => (id, bound),
            RunResolved::Mismatch(run) => return Ok(Err(mismatch(&run, agent, agent_session))),
        };
        let mut cleared = None;
        if let Some(bound) = bound {
            let (event, attrs) = &bound.event;
            self.bus
                .publish(Notice::new(*event, crate::events::RUN_BOUND, attrs));
            // the agent left the session it was in: on /clear or a fork,
            // and on any new conversation, as agy starts after /clear
            // without saying why
            cleared = bound.previous;
        }
        Ok(Ok(Admitted { session, cleared }))
    }
}

fn mismatch(run: &RunRow, agent: &str, agent_session: &str) -> Mismatch {
    let theirs = match (&run.session, &run.agent_session) {
        (Some((a, _)), Some(id)) => format!("session {id} of {a}"),
        (Some((a, id)), None) => format!("session {id} of {a}"),
        (None, _) => "no session yet".to_string(),
    };
    Mismatch(format!(
        "the report is for session {agent_session} of {agent}, but its run {} is in {theirs}",
        run.id
    ))
}

/// Whether `reported` is the run's own agent process: the process the
/// host spawned for it, by pid and start time. When the spawned process is
/// a launcher rather than the agent, as the npm `codex` is, its direct
/// child counts too. `agent` tells whether a process is the agent's own,
/// as its adapter recognises it, and `lookup` reads a running process.
pub(crate) fn own_process(
    run: &RunRow,
    reported: Option<&(i64, String)>,
    agent: &dyn Fn(&Process) -> bool,
    lookup: &dyn Fn(u32) -> Option<Process>,
) -> bool {
    let (Some((pid, started)), Some((reported_pid, reported_started))) = (&run.process, reported)
    else {
        return false;
    };
    if pid == reported_pid && started == reported_started {
        return true;
    }
    let (Ok(spawned), Ok(child)) = (u32::try_from(*pid), u32::try_from(*reported_pid)) else {
        return false;
    };
    let launcher = lookup(spawned).is_some_and(|p| &p.started == started && !agent(&p));
    launcher && lookup(child).is_some_and(|p| &p.started == reported_started && p.parent == spawned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(process: Option<(i64, &str)>) -> RunRow {
        RunRow {
            id: "run_1".into(),
            host: "local".into(),
            agent: "codex".into(),
            kind: "start".into(),
            terminal: Some("t_1".into()),
            process: process.map(|(pid, started)| (pid, started.to_string())),
            folder: "/".into(),
            state: "running".into(),
            end_reason: None,
            exit_code: None,
            exit_signal: None,
            created_by: "app".into(),
            started_at: String::new(),
            ended_at: None,
            session: None,
            agent_session: None,
        }
    }

    fn process(pid: u32, parent: u32, name: &str, started: &str) -> Process {
        Process {
            pid,
            parent,
            name: name.into(),
            started: started.into(),
        }
    }

    /// A launcher `node` (10) that started `codex` (11), which started a
    /// shell (12) that started another `codex` (13).
    fn lookup(pid: u32) -> Option<Process> {
        match pid {
            10 => Some(process(10, 1, "node", "s10")),
            11 => Some(process(11, 10, "codex", "s11")),
            12 => Some(process(12, 11, "zsh", "s12")),
            13 => Some(process(13, 12, "codex", "s13")),
            _ => None,
        }
    }

    fn codex(p: &Process) -> bool {
        p.name == "codex"
    }

    #[test]
    fn the_spawned_process_is_the_runs_own() {
        let run = run(Some((11, "s11")));
        assert!(own_process(
            &run,
            Some(&(11, "s11".into())),
            &codex,
            &lookup
        ));
        // the same pid, reused by another process
        assert!(!own_process(
            &run,
            Some(&(11, "other".into())),
            &codex,
            &lookup
        ));
        assert!(!own_process(&run, None, &codex, &lookup));
    }

    #[test]
    fn a_launchers_direct_child_is_the_runs_own() {
        let run = run(Some((10, "s10")));
        assert!(own_process(
            &run,
            Some(&(11, "s11".into())),
            &codex,
            &lookup
        ));
        // a grandchild is a nested agent
        assert!(!own_process(
            &run,
            Some(&(13, "s13".into())),
            &codex,
            &lookup
        ));
    }

    #[test]
    fn an_agents_own_child_is_not_the_runs() {
        // the spawned process is the agent itself: its children are the
        // tools it runs, and an agent among them is a nested one
        let run = run(Some((11, "s11")));
        assert!(!own_process(
            &run,
            Some(&(12, "s12".into())),
            &codex,
            &lookup
        ));
    }
}
