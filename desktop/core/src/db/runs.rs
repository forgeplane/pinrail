//! The agent processes Pinrail starts in a host's terminals: one row per
//! run, the command it started with, the sessions it has been in and the
//! digest of its token. A run is written as `starting` before the host
//! spawns it, becomes `running` once the host reports its terminal and
//! process, and ends once.

use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{Map, Value, json};

use super::Db;
use super::events::{EventActor, EventSubject, NewEvent, insert_event_as};
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
    /// The session the run is in now, as `(agent, id)`, by Pinrail's id.
    pub session: Option<(String, String)>,
    /// The agent's own id for that session, once the agent reported it.
    pub agent_session: Option<String>,
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
            "session": self.session_json(),
        })
    }

    /// The session the run is in now, as the API shows it, or null.
    pub fn session_json(&self) -> Value {
        match &self.session {
            Some((agent, id)) => {
                json!({"agent": agent, "id": id, "agent_session": self.agent_session})
            }
            None => Value::Null,
        }
    }
}

/// One session a run has been in.
#[derive(Debug, Clone, PartialEq)]
pub struct RunSession {
    pub agent: String,
    pub id: String,
    pub agent_session: Option<String>,
    pub bound_at: String,
    pub unbound_at: Option<String>,
}

impl RunSession {
    pub fn to_json(&self) -> Value {
        json!({
            "agent": self.agent,
            "id": self.id,
            "agent_session": self.agent_session,
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
    /// The session the run starts in, by Pinrail's id: created and shown
    /// if it is new.
    pub session: Option<&'a str>,
    /// The agent's own id for that session, when Pinrail chose it, as it
    /// does for Claude Code; otherwise the agent's first report gives it.
    pub agent_session: Option<&'a str>,
    /// The git checkout the folder is in, which gives the session its
    /// repository and branch.
    pub checkout: Option<&'a pinrail_runtime::checkout::Checkout>,
}

/// What a report sent with a run's token is for.
#[derive(Debug, Clone, PartialEq)]
pub enum RunResolved {
    /// The session, by Pinrail's id, whether it was created, and the run's
    /// move to it when the report moved it.
    Session {
        id: String,
        created: bool,
        bound: Option<Bound>,
    },
    /// Another session than the run's, which the report does not start:
    /// the run as it is.
    Mismatch(Box<RunRow>),
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
    /// The session it was in before, as `(agent, id)`, by Pinrail's id.
    pub previous: Option<(String, String)>,
    pub event: Stored,
}

const COLUMNS: &str = "r.id, r.host_id, r.agent, r.kind, r.terminal_id, r.pid, r.proc_started,
     r.folder, r.state, r.end_reason, r.exit_code, r.exit_signal, r.created_by, r.started_at,
     r.ended_at, s.agent, s.session, ss.agent_session";

/// The run's current session: the binding not yet ended.
const CURRENT: &str = "LEFT JOIN run_sessions s ON s.run_id = r.id AND s.unbound_at IS NULL
     LEFT JOIN sessions ss ON ss.agent = s.agent AND ss.id = s.session";

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
        agent_session: r.get(17)?,
    })
}

/// Moves the live run `row` to the session `session`, by Pinrail's id,
/// which is created and shown if it is new, and ends its binding to the
/// session it was in. Announces `run_bound` with that previous session.
/// `None` when the run is already in this session, or is not live.
fn bind_in(
    tx: &Connection,
    row: &RunRow,
    session: &str,
    origin: &Map<String, Value>,
    now: DateTime<Utc>,
) -> rusqlite::Result<Option<Bound>> {
    if !row.is_live()
        || row
            .session
            .as_ref()
            .is_some_and(|(_, current)| current == session)
    {
        return Ok(None);
    }
    let at = activity(now);
    tx.execute(
        "UPDATE run_sessions SET unbound_at = ?2 WHERE run_id = ?1 AND unbound_at IS NULL",
        params![row.id, at],
    )?;
    super::sessions::touch(tx, &row.agent, session, origin, now)?;
    // a run that comes back to a session it was in before binds again
    tx.execute(
        "INSERT INTO run_sessions (run_id, agent, session, bound_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (run_id, agent, session) DO UPDATE SET bound_at = excluded.bound_at,
           unbound_at = NULL",
        params![row.id, row.agent, session, at],
    )?;
    let previous = row.session.clone();
    let agent_session: Option<String> = tx
        .query_row(
            "SELECT agent_session FROM sessions WHERE agent = ?1 AND id = ?2",
            params![row.agent, session],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let attrs = json!({
        "run": row.id,
        "session": {"agent": row.agent, "id": session, "agent_session": agent_session},
        "previous": match &previous {
            Some((agent, id)) => {
                json!({"agent": agent, "id": id, "agent_session": row.agent_session})
            }
            None => Value::Null,
        },
    });
    let event = run_event(tx, crate::events::RUN_BOUND, Some(session), &attrs)?;
    Ok(Some(Bound {
        previous,
        event: (event, attrs),
    }))
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
    /// The sessions of `repo` that a live run is in, as agent and id.
    pub fn live_sessions_in_repo(&self, repo: &str) -> rusqlite::Result<Vec<(String, String)>> {
        let conn = self.read();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT s.agent, s.id FROM sessions s
               JOIN repos g ON g.id = s.repo_id
               JOIN run_sessions rs ON rs.agent = s.agent AND rs.session = s.id
                                   AND rs.unbound_at IS NULL
               JOIN runs r ON r.id = rs.run_id
              WHERE g.name = ?1 AND r.state != 'ended'",
        )?;
        let rows = stmt.query_map(params![repo], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

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
            if let Some(checkout) = run.checkout {
                for (key, value) in [("repo", &checkout.repo), ("ref", &checkout.reference)] {
                    if let Some(value) = value {
                        origin.insert(key.to_string(), json!(value));
                    }
                }
            }
            super::sessions::touch(&tx, run.agent, session, &origin, now)?;
            if let Some(agent_session) = run.agent_session {
                tx.execute(
                    "UPDATE sessions SET agent_session = ?3
                     WHERE agent = ?1 AND id = ?2 AND agent_session IS NULL",
                    params![run.agent, session, agent_session],
                )?;
            }
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
        let session = row.session.as_ref().map(|(_, id)| id.as_str());
        let event = run_event(&tx, crate::events::RUN_STARTED, session, &attrs)?;
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
        let session = row.session.as_ref().map(|(_, id)| id.as_str());
        let event = run_event(&tx, crate::events::RUN_ENDED, session, &attrs)?;
        tx.commit()?;
        Ok(Some((row, (event, attrs))))
    }

    /// Moves a live run to the session `session`, by Pinrail's id, which is
    /// created and shown if it is new, and ends its binding to the session it was in.
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
        let Some(row) = get(&tx, run)? else {
            return Ok(None);
        };
        let bound = bind_in(&tx, &row, session, origin, now)?;
        tx.commit()?;
        Ok(bound)
    }

    /// The session a report sent with the token of the run `run` is for,
    /// by Pinrail's id, from the agent's own id for it, `agent_session`:
    ///
    /// - the run's session, when its agent's id is this one, or is not
    ///   known yet, which the report then gives it (or moves a live run to
    ///   the existing session that already has this id);
    /// - otherwise, for a report that starts a session (`starts`), the
    ///   session with this id, created if it is new, to which the run
    ///   moves, as on `/clear` or a fork;
    /// - otherwise none: the report is for another session than the run's.
    pub fn resolve_run_session(
        &self,
        run: &str,
        agent: &str,
        agent_session: &str,
        starts: bool,
        origin: &Map<String, Value>,
        now: DateTime<Utc>,
    ) -> rusqlite::Result<RunResolved> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let Some(row) = get(&tx, run)? else {
            let (id, created) = super::sessions::find_or_create(&tx, agent, agent_session, now)?;
            tx.commit()?;
            return Ok(RunResolved::Session {
                id,
                created,
                bound: None,
            });
        };
        if row.agent != agent {
            return Ok(RunResolved::Mismatch(Box::new(row)));
        }
        if let Some((_, current)) = &row.session {
            if row.agent_session.as_deref() == Some(agent_session) {
                return Ok(RunResolved::Session {
                    id: current.clone(),
                    created: false,
                    bound: None,
                });
            }
            // the agent names the session Pinrail started for the first time
            if row.agent_session.is_none() {
                if let Some(existing) = super::sessions::find(&tx, agent, agent_session)? {
                    if row.is_live() {
                        let bound = bind_in(&tx, &row, &existing, origin, now)?;
                        tx.commit()?;
                        return Ok(RunResolved::Session {
                            id: existing,
                            created: false,
                            bound,
                        });
                    }
                } else {
                    tx.execute(
                        "UPDATE sessions SET agent_session = ?3 WHERE agent = ?1 AND id = ?2",
                        params![agent, current, agent_session],
                    )?;
                    tx.commit()?;
                    return Ok(RunResolved::Session {
                        id: current.clone(),
                        created: false,
                        bound: None,
                    });
                }
            }
        }
        if !starts || !row.is_live() {
            return Ok(RunResolved::Mismatch(Box::new(row)));
        }
        let (id, created) = super::sessions::find_or_create(&tx, agent, agent_session, now)?;
        let bound = bind_in(&tx, &row, &id, origin, now)?;
        tx.commit()?;
        Ok(RunResolved::Session { id, created, bound })
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

    /// The command a run started its agent with.
    pub fn run_argv(&self, id: &str) -> rusqlite::Result<Option<Vec<String>>> {
        use rusqlite::OptionalExtension;
        self.conn()
            .query_row(
                "SELECT argv FROM run_launches WHERE run_id = ?1",
                [id],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map(|argv| argv.map(|a| serde_json::from_str(&a).unwrap_or_default()))
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

    pub fn run(&self, id: &str) -> rusqlite::Result<Option<RunRow>> {
        get(&self.read(), id)
    }

    /// The sessions a run has been in, in the order it was bound to them.
    pub fn run_sessions(&self, id: &str) -> rusqlite::Result<Vec<RunSession>> {
        let conn = self.read();
        let mut stmt = conn.prepare(
            "SELECT b.agent, b.session, b.bound_at, b.unbound_at, s.agent_session
             FROM run_sessions b LEFT JOIN sessions s ON s.agent = b.agent AND s.id = b.session
             WHERE b.run_id = ?1 ORDER BY b.bound_at, b.rowid",
        )?;
        stmt.query_map(params![id], |r| {
            Ok(RunSession {
                agent: r.get(0)?,
                id: r.get(1)?,
                bound_at: r.get(2)?,
                unbound_at: r.get(3)?,
                agent_session: r.get(4)?,
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
               AND (?3 IS NULL OR EXISTS (SELECT 1 FROM run_sessions b
                     WHERE b.run_id = r.id AND b.session = ?3 AND (?2 = '' OR b.agent = ?2)))
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

/// Records an event about a run, which Pinrail starts and ends: about its
/// session, when the run is in one.
fn run_event(
    conn: &Connection,
    kind: &str,
    session: Option<&str>,
    attrs: &Value,
) -> rusqlite::Result<i64> {
    insert_event_as(
        conn,
        &NewEvent {
            kind,
            subject: session.map(|id| EventSubject::Session(id.to_string())),
            actor: EventActor::System,
            attrs,
        },
    )
}
