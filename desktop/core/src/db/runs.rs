//! The agent processes Pinrail starts in a host's terminals: one row per
//! run, the command it started with, the sessions it has been in and the
//! digest of its token. A run is written as `starting` before the host
//! spawns it, becomes `running` once the host reports its terminal and
//! process, and ends once.

use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{Map, Value, json};

use super::Db;
use super::events::insert_event;
use super::sessions::activity;

/// How long a run's token still counts after the run ended, so that the
/// last reports of the exiting agent are its run's.
pub const TOKEN_GRACE: Duration = Duration::minutes(5);

/// A run as stored, with the session it is in now.
#[derive(Debug, Clone, PartialEq)]
pub struct RunRow {
    pub id: String,
    pub host: String,
    pub agent: String,
    /// `start` or `resume`
    pub kind: String,
    pub terminal: Option<String>,
    /// The run's own agent process: its pid and its start time.
    pub process: Option<(i64, String)>,
    pub folder: String,
    /// `starting`, `running` or `ended`
    pub state: String,
    pub end_reason: Option<String>,
    pub exit_code: Option<i64>,
    pub exit_signal: Option<String>,
    /// `app` or `cli`
    pub created_by: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    /// The session the run is in now, as `(agent, id)`.
    pub session: Option<(String, String)>,
}

impl RunRow {
    pub fn is_live(&self) -> bool {
        self.state != "ended"
    }

    /// The run as the API shows it. The terminal is named with its host,
    /// as `local:t_…`.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "host": self.host,
            "agent": self.agent,
            "kind": self.kind,
            "state": self.state,
            "terminal": self.terminal.as_ref().map(|t| format!("{}:{t}", self.host)),
            "folder": self.folder,
            "created_by": self.created_by,
            "started_at": self.started_at,
            "ended_at": self.ended_at,
            "end_reason": self.end_reason,
            "exit_code": self.exit_code,
            "exit_signal": self.exit_signal,
            "session": self.session.as_ref().map(|(agent, id)| json!({"agent": agent, "id": id})),
        })
    }
}

/// One session a run has been in.
#[derive(Debug, Clone, PartialEq)]
pub struct RunSession {
    pub agent: String,
    pub id: String,
    pub bound_at: String,
    pub unbound_at: Option<String>,
}

impl RunSession {
    pub fn to_json(&self) -> Value {
        json!({
            "agent": self.agent,
            "id": self.id,
            "bound_at": self.bound_at,
            "unbound_at": self.unbound_at,
        })
    }
}

/// A run about to be spawned.
/// The first task and the extra arguments a run was started with.
pub type RunLaunch = (Option<String>, Vec<String>);

#[derive(Debug, Clone)]
pub struct NewRun<'a> {
    pub id: &'a str,
    pub host: &'a str,
    pub agent: &'a str,
    pub kind: &'a str,
    pub folder: &'a str,
    pub argv: &'a [String],
    /// The first task and the extra arguments the person gave, kept to
    /// start the run again.
    pub task: Option<&'a str>,
    pub args: &'a [String],
    pub created_by: &'a str,
    /// The SHA-256 digest of the run's token, in hex.
    pub token_digest: &'a str,
    /// The session the run starts in, when Pinrail chose its id or resumes
    /// it: created and shown if it is new.
    pub session: Option<&'a str>,
}

/// Which runs to list.
#[derive(Debug, Clone, Default)]
pub struct RunFilters {
    /// Only live runs (`Some(true)`), or only ended ones (`Some(false)`).
    pub live: Option<bool>,
    /// Only the runs that have been in this session, as `(agent, id)`.
    pub session: Option<(String, String)>,
    pub host: Option<String>,
}

/// A stored event, to publish once the change is committed.
pub type Stored = (i64, Value);

/// A run moved to a session.
#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    /// The session it was in before, as `(agent, id)`.
    pub previous: Option<(String, String)>,
    pub event: Stored,
}

const COLUMNS: &str = "r.id, r.host_id, r.agent, r.kind, r.terminal_id, r.pid, r.proc_started,
     r.folder, r.state, r.end_reason, r.exit_code, r.exit_signal, r.created_by, r.started_at,
     r.ended_at, s.agent, s.session";

/// The run's current session: the binding not yet ended.
const CURRENT: &str = "LEFT JOIN run_sessions s ON s.run_id = r.id AND s.unbound_at IS NULL";

fn from_row(r: &Row) -> rusqlite::Result<RunRow> {
    let pid: Option<i64> = r.get(5)?;
    let started: Option<String> = r.get(6)?;
    let agent: Option<String> = r.get(15)?;
    let session: Option<String> = r.get(16)?;
    Ok(RunRow {
        id: r.get(0)?,
        host: r.get(1)?,
        agent: r.get(2)?,
        kind: r.get(3)?,
        terminal: r.get(4)?,
        process: pid.zip(started),
        folder: r.get(7)?,
        state: r.get(8)?,
        end_reason: r.get(9)?,
        exit_code: r.get(10)?,
        exit_signal: r.get(11)?,
        created_by: r.get(12)?,
        started_at: r.get(13)?,
        ended_at: r.get(14)?,
        session: agent.zip(session),
    })
}

fn get(conn: &Connection, id: &str) -> rusqlite::Result<Option<RunRow>> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM runs r {CURRENT} WHERE r.id = ?1"),
        params![id],
        from_row,
    )
    .optional()
}

impl Db {
    /// Records a run as `starting`, with its command and the digest of its
    /// token, before the host is asked to spawn it. A run that names its
    /// session is bound to it from the start, and the session is created
    /// and shown if it is new.
    pub fn insert_run(&self, run: &NewRun, now: DateTime<Utc>) -> rusqlite::Result<RunRow> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let at = activity(now);
        tx.execute(
            "INSERT INTO runs (id, host_id, agent, kind, folder, state, created_by, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'starting', ?6, ?7)",
            params![
                run.id,
                run.host,
                run.agent,
                run.kind,
                run.folder,
                run.created_by,
                at
            ],
        )?;
        tx.execute(
            "INSERT INTO run_launches (run_id, argv, task, args) VALUES (?1, ?2, ?3, ?4)",
            params![
                run.id,
                json!(run.argv).to_string(),
                run.task,
                json!(run.args).to_string()
            ],
        )?;
        tx.execute(
            "INSERT INTO run_tokens (run_id, digest) VALUES (?1, ?2)",
            params![run.id, run.token_digest],
        )?;
        if let Some(session) = run.session {
            let mut origin = Map::new();
            origin.insert("folder".to_string(), json!(run.folder));
            super::sessions::touch(&tx, run.agent, session, &origin, now)?;
            tx.execute(
                "INSERT INTO run_sessions (run_id, agent, session, bound_at) VALUES (?1, ?2, ?3, ?4)",
                params![run.id, run.agent, session, at],
            )?;
        }
        let row = get(&tx, run.id)?.expect("the run just written");
        tx.commit()?;
        Ok(row)
    }

    /// Records that the host spawned a starting run: its terminal and its
    /// agent process. Announces `run_started`. `None` when the run is not
    /// starting.
    pub fn run_spawned(
        &self,
        id: &str,
        terminal: &str,
        process: Option<(i64, String)>,
    ) -> rusqlite::Result<Option<(RunRow, Stored)>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let (pid, started) = process.unzip();
        let changed = tx.execute(
            "UPDATE runs SET state = 'running', terminal_id = ?2, pid = ?3, proc_started = ?4
             WHERE id = ?1 AND state = 'starting'",
            params![id, terminal, pid, started],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        let row = get(&tx, id)?.expect("the run just changed");
        let attrs = json!({"run": row.to_json()});
        let event = insert_event(&tx, None, crate::events::RUN_STARTED, None, &attrs)?;
        tx.commit()?;
        Ok(Some((row, (event, attrs))))
    }

    /// Ends a live run for `reason`, with how its process exited when that
    /// is known. Announces `run_ended`. `None` when the run has already
    /// ended or does not exist.
    pub fn end_run(
        &self,
        id: &str,
        reason: &str,
        exit_code: Option<i64>,
        exit_signal: Option<&str>,
        now: DateTime<Utc>,
    ) -> rusqlite::Result<Option<(RunRow, Stored)>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE runs SET state = 'ended', end_reason = ?2, exit_code = ?3,
               exit_signal = ?4, ended_at = ?5
             WHERE id = ?1 AND state != 'ended'",
            params![id, reason, exit_code, exit_signal, activity(now)],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        let row = get(&tx, id)?.expect("the run just ended");
        let attrs = json!({"run": row.to_json(), "reason": reason, "exit_code": exit_code});
        let event = insert_event(&tx, None, crate::events::RUN_ENDED, None, &attrs)?;
        tx.commit()?;
        Ok(Some((row, (event, attrs))))
    }

    /// Moves a live run to the session `(agent, id)`, which is created and
    /// shown if it is new, and ends its binding to the session it was in.
    /// Announces `run_bound` with that previous session. `None` when the
    /// run is already in this session, or is not live.
    pub fn bind_run(
        &self,
        run: &str,
        session: &str,
        origin: &Map<String, Value>,
        now: DateTime<Utc>,
    ) -> rusqlite::Result<Option<Bound>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let Some(row) = get(&tx, run)?.filter(RunRow::is_live) else {
            return Ok(None);
        };
        if row
            .session
            .as_ref()
            .is_some_and(|(_, current)| current == session)
        {
            return Ok(None);
        }
        let at = activity(now);
        tx.execute(
            "UPDATE run_sessions SET unbound_at = ?2 WHERE run_id = ?1 AND unbound_at IS NULL",
            params![run, at],
        )?;
        super::sessions::touch(&tx, &row.agent, session, origin, now)?;
        // a run that comes back to a session it was in before binds again
        tx.execute(
            "INSERT INTO run_sessions (run_id, agent, session, bound_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (run_id, agent, session) DO UPDATE SET bound_at = excluded.bound_at,
               unbound_at = NULL",
            params![run, row.agent, session, at],
        )?;
        let previous = row.session.clone();
        let attrs = json!({
            "run": row.id,
            "session": {"agent": row.agent, "id": session},
            "previous": previous.as_ref().map(|(agent, id)| json!({"agent": agent, "id": id})),
        });
        let event = insert_event(&tx, None, crate::events::RUN_BOUND, None, &attrs)?;
        tx.commit()?;
        Ok(Some(Bound {
            previous,
            event: (event, attrs),
        }))
    }

    /// The folders of this computer's runs and of sessions, most recently
    /// used first, each with the repository its newest session named, and
    /// when it was last used.
    pub fn recent_folders(
        &self,
        limit: usize,
    ) -> rusqlite::Result<Vec<(String, Option<String>, String)>> {
        let conn = self.conn();
        let repo = super::repos::name_of("s.repo_id");
        let mut stmt = conn.prepare(&format!(
            "WITH used AS (
               SELECT s.folder AS folder, s.last_at AS at, {repo} AS repo FROM sessions s
               WHERE s.folder IS NOT NULL AND s.folder <> ''
               UNION ALL
               SELECT folder, started_at, NULL FROM runs WHERE host_id = 'local'
             )
             SELECT u.folder, MAX(u.at) AS at,
               (SELECT v.repo FROM used v WHERE v.folder = u.folder AND v.repo IS NOT NULL
                ORDER BY v.at DESC LIMIT 1)
             FROM used u GROUP BY u.folder ORDER BY at DESC LIMIT ?1"
        ))?;
        stmt.query_map([limit as i64], |r| Ok((r.get(0)?, r.get(2)?, r.get(1)?)))?
            .collect()
    }

    /// The first task and the extra arguments a run was started with.
    pub fn run_launch(&self, id: &str) -> rusqlite::Result<Option<RunLaunch>> {
        use rusqlite::OptionalExtension;
        self.conn()
            .query_row(
                "SELECT task, args FROM run_launches WHERE run_id = ?1",
                [id],
                |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()
            .map(|launch| {
                launch.map(|(task, args)| (task, serde_json::from_str(&args).unwrap_or_default()))
            })
    }

    /// The agent of the latest run, if there is one.
    pub fn latest_run_agent(&self) -> rusqlite::Result<Option<String>> {
        use rusqlite::OptionalExtension;
        self.conn()
            .query_row(
                "SELECT agent FROM runs ORDER BY started_at DESC, id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
    }

    pub fn run(&self, id: &str) -> rusqlite::Result<Option<RunRow>> {
        get(&self.read(), id)
    }

    /// The sessions a run has been in, in the order it was bound to them.
    pub fn run_sessions(&self, id: &str) -> rusqlite::Result<Vec<RunSession>> {
        let conn = self.read();
        let mut stmt = conn.prepare(
            "SELECT agent, session, bound_at, unbound_at FROM run_sessions
             WHERE run_id = ?1 ORDER BY bound_at, rowid",
        )?;
        stmt.query_map(params![id], |r| {
            Ok(RunSession {
                agent: r.get(0)?,
                id: r.get(1)?,
                bound_at: r.get(2)?,
                unbound_at: r.get(3)?,
            })
        })?
        .collect()
    }

    /// The boot the core last saw the host in, if it saw it at all.
    pub fn host_boot(&self, host: &str) -> rusqlite::Result<Option<String>> {
        self.read()
            .query_row(
                "SELECT boot_id FROM hosts WHERE id = ?1",
                params![host],
                |r| r.get(0),
            )
            .optional()
            .map(Option::flatten)
    }

    /// Records the boot the core sees the host in now.
    pub fn set_host_boot(&self, host: &str, boot: &str) -> rusqlite::Result<()> {
        self.conn().execute(
            "UPDATE hosts SET boot_id = ?2 WHERE id = ?1",
            params![host, boot],
        )?;
        Ok(())
    }

    /// The runs a restart of the computer ended, latest first, whose
    /// session no run has resumed since, and which ended after the person
    /// last dismissed them.
    pub fn restarted_runs(&self, host: &str) -> rusqlite::Result<Vec<RunRow>> {
        let conn = self.read();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM runs r {CURRENT}
               JOIN hosts h ON h.id = r.host_id
             WHERE r.host_id = ?1 AND r.end_reason = 'restart'
               AND (h.restart_seen_at IS NULL OR r.ended_at > h.restart_seen_at)
               AND NOT EXISTS (SELECT 1 FROM run_sessions later
                     JOIN runs lr ON lr.id = later.run_id
                     WHERE later.agent = s.agent AND later.session = s.session
                       AND lr.id != r.id AND lr.started_at >= r.ended_at)
             ORDER BY r.ended_at DESC, r.id DESC"
        ))?;
        stmt.query_map(params![host], from_row)?.collect()
    }

    /// Hides the runs a restart ended until the next restart.
    pub fn dismiss_restart(&self, host: &str, now: DateTime<Utc>) -> rusqlite::Result<()> {
        self.conn().execute(
            "UPDATE hosts SET restart_seen_at = ?2 WHERE id = ?1",
            params![host, activity(now)],
        )?;
        Ok(())
    }

    /// The runs the filters select, the latest first.
    pub fn runs(&self, filters: &RunFilters) -> rusqlite::Result<Vec<RunRow>> {
        let conn = self.read();
        let (agent, session) = filters.session.clone().unzip();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM runs r {CURRENT}
             WHERE (?1 IS NULL OR (?1 = 1) = (r.state != 'ended'))
               AND (?2 IS NULL OR EXISTS (SELECT 1 FROM run_sessions b
                     WHERE b.run_id = r.id AND b.agent = ?2 AND b.session = ?3))
               AND (?4 IS NULL OR r.host_id = ?4)
             ORDER BY r.started_at DESC, r.id DESC"
        ))?;
        stmt.query_map(
            params![filters.live, agent, session, filters.host],
            from_row,
        )?
        .collect()
    }

    /// The run whose token has this digest, while the token counts: as
    /// long as the run is live, and for 5 minutes after it ended.
    pub fn run_for_token(
        &self,
        digest: &str,
        now: DateTime<Utc>,
    ) -> rusqlite::Result<Option<RunRow>> {
        let conn = self.read();
        let id: Option<String> = conn
            .query_row(
                "SELECT r.id FROM run_tokens t JOIN runs r ON r.id = t.run_id
                 WHERE t.digest = ?1 AND (r.state != 'ended' OR r.ended_at > ?2)",
                params![digest, activity(now - TOKEN_GRACE)],
                |r| r.get(0),
            )
            .optional()?;
        match id {
            Some(id) => get(&conn, &id),
            None => Ok(None),
        }
    }

    /// Deletes the digests of the tokens of runs that ended more than 5
    /// minutes before `now`. Returns how many it deleted.
    pub fn forget_run_tokens(&self, now: DateTime<Utc>) -> rusqlite::Result<usize> {
        self.conn().execute(
            "DELETE FROM run_tokens WHERE run_id IN
               (SELECT id FROM runs WHERE state = 'ended' AND ended_at <= ?1)",
            params![activity(now - TOKEN_GRACE)],
        )
    }
}
