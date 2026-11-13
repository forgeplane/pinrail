//! What an agent's hooks report about its session: the state it is in now,
//! one row per session, and its turns, with the prompt and the reply of
//! each when the person keeps them. A session known first from a report is
//! hidden until it reports a status, asks or submits.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value};

use super::Db;
use super::sessions::activity;

/// The longest prompt and reply kept of a turn, in characters.
pub const PROMPT_CHARS: usize = 4_000;
pub const REPLY_CHARS: usize = 8_000;

/// One report, as the core received it.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub event: ReportEvent,
    /// Orders the reports of a session: one not greater than the last
    /// accepted is older, and is ignored.
    pub seq: i64,
    pub at: DateTime<Utc>,
    /// The agent's process: its pid and when it started.
    pub process: Option<(i64, String)>,
    /// Where the session works: `repo`, `ref` and `folder`, as a status's
    /// origin gives them, used when the report creates the session.
    pub origin: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReportEvent {
    Started,
    Prompt {
        prompt: Option<String>,
    },
    Working,
    NeedsInput {
        kind: String,
        tool: Option<String>,
        summary: Option<String>,
    },
    TurnEnded {
        end: String,
        reply: Option<String>,
    },
    Ended {
        reason: Option<String>,
    },
}

/// A session's presence, as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct PresenceRow {
    pub state: String,
    pub wait_kind: Option<String>,
    pub wait_tool: Option<String>,
    pub wait_summary: Option<String>,
    pub stop_reason: Option<String>,
    pub source: String,
    pub pid: Option<i64>,
    pub seq: i64,
    pub state_since: DateTime<Utc>,
    pub turn: i64,
    pub turn_started: Option<DateTime<Utc>>,
    pub seen_turn: i64,
}

/// A turn of a session, with its texts when they were kept.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnRow {
    pub turn: i64,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub end_kind: Option<String>,
    pub prompt: Option<String>,
    pub reply: Option<String>,
}

/// What a report needs of the presence stored before it.
struct Stored {
    seq: i64,
    state: String,
    wait_kind: Option<String>,
    state_since: String,
    turn: i64,
    turn_started: Option<String>,
}

/// The state a report leaves: the state, what it waits for, why it stopped.
struct Next {
    state: &'static str,
    wait: Option<(String, Option<String>, Option<String>)>,
    stop_reason: Option<&'static str>,
}

fn next(event: &ReportEvent) -> Next {
    let plain = |state| Next {
        state,
        wait: None,
        stop_reason: None,
    };
    match event {
        ReportEvent::Started | ReportEvent::TurnEnded { .. } => plain("idle"),
        ReportEvent::Prompt { .. } | ReportEvent::Working => plain("working"),
        ReportEvent::NeedsInput {
            kind,
            tool,
            summary,
        } => Next {
            state: "needs_input",
            wait: Some((kind.clone(), tool.clone(), summary.clone())),
            stop_reason: None,
        },
        ReportEvent::Ended { reason } => Next {
            state: "stopped",
            wait: None,
            // Claude Code ends the session it clears with `clear`
            stop_reason: Some(match reason.as_deref() {
                Some("clear") => "cleared",
                _ => "ended",
            }),
        },
    }
}

/// At most `n` characters of a text.
fn cut(text: &Option<String>, n: usize) -> Option<String> {
    text.as_ref().map(|t| t.chars().take(n).collect())
}

impl Db {
    /// Records a report of the session's hooks, creating the session, hidden,
    /// if it is new. Returns whether it was taken: a report whose `seq` is
    /// not greater than the last one's changes nothing. A prompt opens a
    /// turn and counts as the session's activity; the end of a turn or of
    /// the session closes it. The texts are kept when `keep_texts` is set.
    pub fn record_report(
        &self,
        agent: &str,
        id: &str,
        report: &Report,
        keep_texts: bool,
    ) -> rusqlite::Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        create_hidden(&tx, agent, id, &report.origin, report.at)?;
        let current = tx
            .query_row(
                "SELECT seq, state, wait_kind, state_since, turn, turn_started
                 FROM session_presence WHERE agent = ?1 AND session = ?2",
                params![agent, id],
                |r| {
                    Ok(Stored {
                        seq: r.get(0)?,
                        state: r.get(1)?,
                        wait_kind: r.get(2)?,
                        state_since: r.get(3)?,
                        turn: r.get(4)?,
                        turn_started: r.get(5)?,
                    })
                },
            )
            .optional()?;
        if current.as_ref().is_some_and(|c| c.seq >= report.seq) {
            return Ok(false);
        }
        let at = activity(report.at);
        let next = next(&report.event);
        let (mut turn, mut turn_started) = current
            .as_ref()
            .map(|c| (c.turn, c.turn_started.clone()))
            .unwrap_or((0, None));
        let wait_kind = next.wait.as_ref().map(|w| w.0.clone());
        // the state keeps its start while it stays the same
        let state_since = match &current {
            Some(c) if c.state == next.state && c.wait_kind == wait_kind => c.state_since.clone(),
            _ => at.clone(),
        };

        match &report.event {
            ReportEvent::Prompt { prompt } => {
                // a turn left open was cut short: hooks report no interrupt
                close_turn(&tx, agent, id, turn, "interrupted", &at, None, false)?;
                turn += 1;
                turn_started = Some(at.clone());
                tx.execute(
                    "INSERT INTO session_turns (agent, session, turn, started_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![agent, id, turn, at],
                )?;
                if keep_texts && prompt.is_some() {
                    tx.execute(
                        "INSERT INTO session_turn_texts (turn_id, prompt) VALUES (?1, ?2)",
                        params![tx.last_insert_rowid(), cut(prompt, PROMPT_CHARS)],
                    )?;
                }
                // a new turn is the session's activity, and brings it back
                // from the archive
                tx.execute(
                    "UPDATE sessions SET last_at = MAX(last_at, ?3), archived_at = NULL
                     WHERE agent = ?1 AND id = ?2",
                    params![agent, id, at],
                )?;
            }
            ReportEvent::TurnEnded { end, reply } => {
                let reply = cut(reply, REPLY_CHARS);
                close_turn(&tx, agent, id, turn, end, &at, reply, keep_texts)?;
            }
            ReportEvent::Ended { .. } => {
                close_turn(&tx, agent, id, turn, "ended", &at, None, false)?;
            }
            ReportEvent::Started | ReportEvent::Working | ReportEvent::NeedsInput { .. } => {}
        }

        let (wait_tool, wait_summary) = next.wait.map(|w| (w.1, w.2)).unwrap_or((None, None));
        let (pid, started) = report
            .process
            .clone()
            .map(|(pid, started)| (Some(pid), Some(started)))
            .unwrap_or((None, None));
        tx.execute(
            "INSERT INTO session_presence (agent, session, state, wait_kind, wait_tool,
               wait_summary, stop_reason, source, pid, proc_started, seq, state_since,
               turn, turn_started, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'hook', ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT (agent, session) DO UPDATE SET
               state = excluded.state, wait_kind = excluded.wait_kind,
               wait_tool = excluded.wait_tool, wait_summary = excluded.wait_summary,
               stop_reason = excluded.stop_reason, source = excluded.source,
               pid = COALESCE(excluded.pid, pid),
               proc_started = COALESCE(excluded.proc_started, proc_started),
               seq = excluded.seq, state_since = excluded.state_since,
               turn = excluded.turn, turn_started = excluded.turn_started,
               updated_at = excluded.updated_at
             WHERE excluded.seq > session_presence.seq",
            params![
                agent,
                id,
                next.state,
                wait_kind,
                wait_tool,
                wait_summary,
                next.stop_reason,
                pid,
                started,
                report.seq,
                state_since,
                turn,
                turn_started,
                at,
            ],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// The session's presence, if its hooks ever reported.
    pub fn presence(&self, agent: &str, id: &str) -> rusqlite::Result<Option<PresenceRow>> {
        let time = |s: String| crate::reviews::parse_datetime(&s).unwrap_or_default();
        self.read()
            .query_row(
                "SELECT state, wait_kind, wait_tool, wait_summary, stop_reason, source, pid,
                   seq, state_since, turn, turn_started, seen_turn
                 FROM session_presence WHERE agent = ?1 AND session = ?2",
                params![agent, id],
                |r| {
                    Ok(PresenceRow {
                        state: r.get(0)?,
                        wait_kind: r.get(1)?,
                        wait_tool: r.get(2)?,
                        wait_summary: r.get(3)?,
                        stop_reason: r.get(4)?,
                        source: r.get(5)?,
                        pid: r.get(6)?,
                        seq: r.get(7)?,
                        state_since: time(r.get(8)?),
                        turn: r.get(9)?,
                        turn_started: r.get::<_, Option<String>>(10)?.map(time),
                        seen_turn: r.get(11)?,
                    })
                },
            )
            .optional()
    }

    /// The session's turns, oldest first, with their texts.
    pub fn turns(&self, agent: &str, id: &str) -> rusqlite::Result<Vec<TurnRow>> {
        let time = |s: String| crate::reviews::parse_datetime(&s).unwrap_or_default();
        let conn = self.read();
        let mut stmt = conn.prepare(
            "SELECT t.turn, t.started_at, t.ended_at, t.end_kind, x.prompt, x.reply
             FROM session_turns t LEFT JOIN session_turn_texts x ON x.turn_id = t.id
             WHERE t.agent = ?1 AND t.session = ?2 ORDER BY t.turn",
        )?;
        stmt.query_map(params![agent, id], |r| {
            Ok(TurnRow {
                turn: r.get(0)?,
                started_at: time(r.get(1)?),
                ended_at: r.get::<_, Option<String>>(2)?.map(time),
                end_kind: r.get(3)?,
                prompt: r.get(4)?,
                reply: r.get(5)?,
            })
        })?
        .collect()
    }

    /// Deletes the sessions known only from their hooks that reported
    /// nothing since `before`, with all that hangs off them. Returns how
    /// many went.
    pub fn delete_hidden_sessions(&self, before: DateTime<Utc>) -> rusqlite::Result<usize> {
        self.conn().execute(
            "DELETE FROM sessions WHERE hidden = 1 AND COALESCE(
               (SELECT p.updated_at FROM session_presence p
                WHERE p.agent = sessions.agent AND p.session = sessions.id),
               last_at) < ?1",
            params![activity(before)],
        )
    }
}

/// Creates the session the report names, hidden, if there is none yet.
fn create_hidden(
    conn: &Connection,
    agent: &str,
    id: &str,
    origin: &Map<String, Value>,
    at: DateTime<Utc>,
) -> rusqlite::Result<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sessions WHERE agent = ?1 AND id = ?2)",
        params![agent, id],
        |r| r.get(0),
    )?;
    if exists {
        return Ok(());
    }
    let field = |key: &str| origin.get(key).and_then(Value::as_str);
    let repo_id = super::repos::ensure(conn, field("repo"), at)?;
    conn.execute(
        "INSERT INTO sessions (agent, id, repo_id, ref, folder, first_at, last_at, hidden)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 1)",
        params![
            agent,
            id,
            repo_id,
            field("ref"),
            field("folder"),
            activity(at)
        ],
    )?;
    Ok(())
}

/// Ends the session's turn `turn`, if it is still open, as `end`, with the
/// agent's reply when the texts are kept.
#[allow(clippy::too_many_arguments)]
fn close_turn(
    conn: &Connection,
    agent: &str,
    id: &str,
    turn: i64,
    end: &str,
    at: &str,
    reply: Option<String>,
    keep_texts: bool,
) -> rusqlite::Result<()> {
    let open: Option<i64> = conn
        .query_row(
            "SELECT id FROM session_turns
             WHERE agent = ?1 AND session = ?2 AND turn = ?3 AND ended_at IS NULL",
            params![agent, id, turn],
            |r| r.get(0),
        )
        .optional()?;
    let Some(turn_id) = open else {
        return Ok(());
    };
    conn.execute(
        "UPDATE session_turns SET ended_at = ?2, end_kind = ?3 WHERE id = ?1",
        params![turn_id, at, end],
    )?;
    if keep_texts && reply.is_some() {
        conn.execute(
            "INSERT INTO session_turn_texts (turn_id, reply) VALUES (?1, ?2)
             ON CONFLICT (turn_id) DO UPDATE SET reply = excluded.reply",
            params![turn_id, reply],
        )?;
    }
    Ok(())
}
