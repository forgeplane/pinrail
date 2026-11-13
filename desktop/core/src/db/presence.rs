//! What an agent's hooks report about its session: the state it is in now,
//! one row per session, and its turns, with the prompt and the reply of
//! each when the person keeps them. A session known first from a report is
//! hidden until it reports a status, asks or submits.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value, json};

use super::events::insert_event;

use super::Db;
use super::sessions::activity;

/// The longest prompt and reply kept of a turn, in characters.
pub const PROMPT_CHARS: usize = 4_000;
pub const REPLY_CHARS: usize = 32_000;

/// One report, as the core received it.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub event: ReportEvent,
    /// When the agent's hook fired, which orders two reports that arrive
    /// close together, and dates the turn it starts or ends.
    pub at: DateTime<Utc>,
    /// The agent's process: its pid and when it started.
    pub process: Option<(i64, String)>,
    /// Where the session works: `repo`, `ref` and `folder`, as a status's
    /// origin gives them, used when the report creates the session.
    pub origin: Map<String, Value>,
    /// The agent's own transcript of the session.
    pub transcript_path: Option<String>,
    /// The agent's own id for the turn the report is about.
    pub agent_turn: Option<String>,
    /// Whether the report's prompt or reply was cut to its excerpt.
    pub cut: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReportEvent {
    /// The session started, or was resumed, cleared, compacted or forked.
    Started {
        source: Option<String>,
    },
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
    pub event_at: DateTime<Utc>,
    pub state_since: DateTime<Utc>,
    pub turn: i64,
    pub turn_started: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub seen_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

/// A turn of a session, with its texts when they were kept.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnRow {
    pub turn: i64,
    /// The agent's own id for the turn, which places it in the transcript.
    pub agent_turn: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub end_kind: Option<String>,
    pub prompt: Option<String>,
    pub prompt_cut: bool,
    pub reply: Option<String>,
    pub reply_cut: bool,
}

/// How long after the last report a new one is taken whatever its hook's
/// time says: hooks run as separate processes, so two reports that arrive
/// closer than this may arrive out of order, and later ones are newer.
const WINDOW_MICROS: i64 = 2_000_000;

/// The state stored before a report, as the presence service decides
/// from it.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredState {
    pub state: String,
    pub wait_kind: Option<String>,
    pub wait_tool: Option<String>,
    pub wait_summary: Option<String>,
    pub stop_reason: Option<String>,
    pub pid: Option<i64>,
    pub proc_started: Option<String>,
}

impl StoredState {
    /// The session's agent process, as reported: its pid and start time.
    pub fn process(&self) -> Option<(i64, &str)> {
        Some((self.pid?, self.proc_started.as_deref()?))
    }
}

/// What a report needs of the presence stored before it.
struct Stored {
    state: StoredState,
    event_at: String,
    state_at: String,
    state_since: String,
}

/// The state a report sets: the state, what it waits for (its kind, tool and
/// summary), why it stopped.
#[derive(Debug, Clone, PartialEq)]
pub struct Next {
    pub state: String,
    pub wait: Option<(String, Option<String>, Option<String>)>,
    pub stop_reason: Option<String>,
}

/// How the presence service moves a session's state on a report: from the
/// event, the state stored, and the report's process. `None` means the
/// report changes nothing.
pub type Transition<'a> =
    &'a dyn Fn(&ReportEvent, Option<&StoredState>, Option<&(i64, String)>) -> Option<Next>;

/// What a report changed, with the events stored for it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recorded {
    /// Whether it set the session's state, rather than arriving after a
    /// newer report or being ignored.
    pub taken: bool,
    /// `session_presence`, when the state, the wait or the finish changed.
    pub presence: Option<(i64, Value)>,
    /// `session_turn`, when a turn opened or ended.
    pub turn: Option<(i64, Value)>,
}

/// A reply as it is kept: at most its excerpt, and whether it was longer.
struct Excerpt {
    text: Option<String>,
    cut: bool,
}

/// Whether a text is longer than `n` characters.
fn longer(text: &Option<String>, n: usize) -> bool {
    text.as_ref().is_some_and(|t| t.chars().nth(n).is_some())
}

/// At most `n` characters of a text.
fn cut(text: &Option<String>, n: usize) -> Option<String> {
    text.as_ref().map(|t| t.chars().take(n).collect())
}

/// Microseconds between two stored times, `b` after `a`.
fn micros_between(a: &str, b: &str) -> i64 {
    match (
        crate::reviews::parse_datetime(a),
        crate::reviews::parse_datetime(b),
    ) {
        (Some(a), Some(b)) => (b - a).num_microseconds().unwrap_or(i64::MAX),
        _ => i64::MAX,
    }
}

impl Db {
    /// Records a report of the session's hooks at `now`, creating the
    /// session, hidden, if it is new. Returns whether it set the session's
    /// state: a report that arrives within 2 seconds of the one that set it
    /// but fired before that one does not. Turns are kept whatever the order: a prompt
    /// opens one and counts as the session's activity, and the end of a
    /// turn or of the session closes the turn that was open when it fired.
    /// The texts are kept when `keep_texts` is set.
    pub fn record_report(
        &self,
        agent: &str,
        id: &str,
        report: &Report,
        keep_texts: bool,
        now: DateTime<Utc>,
        transition: Transition,
    ) -> rusqlite::Result<Recorded> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let now_at = activity(now);
        // a hook's clock is never later than the core's
        let at = activity(report.at.min(now));
        create_hidden(&tx, agent, id, &report.origin, now)?;
        let stored = tx
            .query_row(
                "SELECT state, wait_kind, wait_tool, wait_summary, stop_reason, event_at,
                   state_at, state_since, pid, proc_started
                 FROM session_presence WHERE agent = ?1 AND session = ?2",
                params![agent, id],
                |r| {
                    Ok(Stored {
                        state: StoredState {
                            state: r.get(0)?,
                            wait_kind: r.get(1)?,
                            wait_tool: r.get(2)?,
                            wait_summary: r.get(3)?,
                            stop_reason: r.get(4)?,
                            pid: r.get(8)?,
                            proc_started: r.get(9)?,
                        },
                        event_at: r.get(5)?,
                        state_at: r.get(6)?,
                        state_since: r.get(7)?,
                    })
                },
            )
            .optional()?;
        let before = read_presence(&tx, agent, id)?;
        let newer = stored.as_ref().is_none_or(|s| {
            at >= s.event_at || micros_between(&s.state_at, &now_at) > WINDOW_MICROS
        });
        // the presence service decides the state; a report it ignores
        // changes nothing at all
        let Some(next) = transition(
            &report.event,
            stored.as_ref().map(|s| &s.state),
            report.process.as_ref(),
        ) else {
            tx.commit()?;
            return Ok(Recorded::default());
        };

        if let Some(path) = &report.transcript_path {
            tx.execute(
                "UPDATE sessions SET transcript_path = ?3 WHERE agent = ?1 AND id = ?2",
                params![agent, id, path],
            )?;
        }
        let turned = record_turns(&tx, agent, id, report, &at, &now_at, keep_texts, !newer)?;

        let finished = matches!(
            &report.event,
            ReportEvent::TurnEnded { end, .. } if end == "done" || end == "failed"
        );
        let (pid, started) = report
            .process
            .clone()
            .map(|(pid, started)| (Some(pid), Some(started)))
            .unwrap_or((None, None));
        let (turn, turn_started): (i64, Option<String>) = tx
            .query_row(
                "SELECT turn, started_at FROM session_turns WHERE agent = ?1 AND session = ?2
                 ORDER BY started_at DESC, turn DESC LIMIT 1",
                params![agent, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .unwrap_or((0, None));

        match (&stored, newer) {
            // older than the state: only the turns, the finish and the
            // process change, and what a wait still lacks
            (Some(s), false) => {
                let fills = match &report.event {
                    ReportEvent::NeedsInput {
                        kind,
                        tool,
                        summary,
                    } if s.state.state == "needs_input"
                        && s.state.wait_kind.as_deref() == Some(kind)
                        && s.state.wait_tool.is_none()
                        && s.state.wait_summary.is_none() =>
                    {
                        (tool.clone(), summary.clone())
                    }
                    _ => (None, None),
                };
                tx.execute(
                    "UPDATE session_presence SET turn = ?3, turn_started = ?4,
                       finished_at = CASE WHEN ?5 THEN ?6 ELSE finished_at END,
                       pid = COALESCE(?7, pid), proc_started = COALESCE(?8, proc_started),
                       wait_tool = COALESCE(wait_tool, ?9),
                       wait_summary = COALESCE(wait_summary, ?10),
                       updated_at = ?6
                     WHERE agent = ?1 AND session = ?2",
                    params![
                        agent,
                        id,
                        turn,
                        turn_started,
                        finished,
                        now_at,
                        pid,
                        started,
                        fills.0,
                        fills.1,
                    ],
                )?;
            }
            _ => {
                let wait_kind = next.wait.as_ref().map(|w| w.0.clone());
                // the state keeps its start while it stays the same
                let state_since = match &stored {
                    Some(s) if s.state.state == next.state && s.state.wait_kind == wait_kind => {
                        s.state_since.clone()
                    }
                    _ => now_at.clone(),
                };
                let (wait_tool, wait_summary) = next
                    .wait
                    .clone()
                    .map(|w| (w.1, w.2))
                    .unwrap_or((None, None));
                tx.execute(
                    "INSERT INTO session_presence (agent, session, state, wait_kind, wait_tool,
                       wait_summary, stop_reason, source, pid, proc_started, event_at,
                       state_at, state_since, turn, turn_started, finished_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'hook', ?8, ?9, ?10, ?15, ?11, ?12,
                       ?13, CASE WHEN ?14 THEN ?15 END, ?15)
                     ON CONFLICT (agent, session) DO UPDATE SET
                       state = excluded.state, wait_kind = excluded.wait_kind,
                       wait_tool = excluded.wait_tool, wait_summary = excluded.wait_summary,
                       stop_reason = excluded.stop_reason, source = excluded.source,
                       pid = COALESCE(excluded.pid, pid),
                       proc_started = COALESCE(excluded.proc_started, proc_started),
                       event_at = excluded.event_at, state_at = excluded.state_at,
                       state_since = excluded.state_since,
                       turn = excluded.turn, turn_started = excluded.turn_started,
                       finished_at = CASE WHEN ?14 THEN ?15 ELSE finished_at END,
                       updated_at = excluded.updated_at",
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
                        at,
                        state_since,
                        turn,
                        turn_started,
                        finished,
                        now_at,
                    ],
                )?;
            }
        }
        // the events of what changed, stored with it
        let mut recorded = Recorded {
            taken: newer,
            ..Recorded::default()
        };
        // announced when anything the window shows changed: the state, the
        // wait and what it is about, the turn, the finish
        let row = read_presence(&tx, agent, id)?;
        let shown = |p: &PresenceRow| {
            (
                p.state.clone(),
                p.wait_kind.clone(),
                p.wait_tool.clone(),
                p.wait_summary.clone(),
                p.stop_reason.clone(),
                p.turn,
                p.turn_started,
                p.finished_at,
            )
        };
        let changed = match (&before, &row) {
            (Some(b), Some(r)) => shown(b) != shown(r),
            _ => true,
        };
        if let Some(row) = row.filter(|_| changed) {
            let attrs = presence_attrs(&tx, agent, id, &row, now)?;
            let event = insert_event(&tx, None, crate::events::SESSION_PRESENCE, None, &attrs)?;
            recorded.presence = Some((event, attrs));
        }
        if let Some((turn, end_kind)) = turned {
            let attrs = json!({"agent": agent, "id": id, "turn": turn, "end_kind": end_kind});
            let event = insert_event(&tx, None, crate::events::SESSION_TURN, None, &attrs)?;
            recorded.turn = Some((event, attrs));
        }
        tx.commit()?;
        Ok(recorded)
    }

    /// Records that the person saw the session at `at`, or, with `None`,
    /// that they marked it unread. Returns `None` when there is no such
    /// session, `Some(None)` when its agent's hooks never reported, which
    /// leaves nothing to see, and otherwise the `session_seen` event with its
    /// attributes.
    #[allow(clippy::type_complexity)]
    pub fn set_seen(
        &self,
        agent: &str,
        id: &str,
        at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> rusqlite::Result<Option<Option<(i64, Value)>>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM sessions WHERE agent = ?1 AND id = ?2)",
            params![agent, id],
            |r| r.get(0),
        )?;
        if !exists {
            return Ok(None);
        }
        let changed = tx.execute(
            "UPDATE session_presence SET seen_at = ?3 WHERE agent = ?1 AND session = ?2",
            params![agent, id, at.map(activity)],
        )?;
        let Some(row) = read_presence(&tx, agent, id)?.filter(|_| changed > 0) else {
            return Ok(Some(None));
        };
        let attrs = presence_attrs(&tx, agent, id, &row, now)?;
        let event = insert_event(&tx, None, crate::events::SESSION_SEEN, None, &attrs)?;
        tx.commit()?;
        Ok(Some(Some((event, attrs))))
    }

    /// The session's presence, if its hooks ever reported.
    pub fn presence(&self, agent: &str, id: &str) -> rusqlite::Result<Option<PresenceRow>> {
        read_presence(&self.read(), agent, id)
    }

    /// The session's turns, oldest first, with their texts.
    pub fn turns(&self, agent: &str, id: &str) -> rusqlite::Result<Vec<TurnRow>> {
        let time = |s: String| crate::reviews::parse_datetime(&s).unwrap_or_default();
        let conn = self.read();
        let mut stmt = conn.prepare(
            "SELECT t.turn, t.agent_turn, t.started_at, t.ended_at, t.end_kind, x.prompt,
               COALESCE(x.prompt_cut, 0), x.reply, COALESCE(x.reply_cut, 0)
             FROM session_turns t LEFT JOIN session_turn_texts x ON x.turn_id = t.id
             WHERE t.agent = ?1 AND t.session = ?2 ORDER BY t.started_at, t.turn",
        )?;
        stmt.query_map(params![agent, id], |r| {
            Ok(TurnRow {
                turn: r.get(0)?,
                agent_turn: r.get(1)?,
                started_at: time(r.get(2)?),
                ended_at: r.get::<_, Option<String>>(3)?.map(time),
                end_kind: r.get(4)?,
                prompt: r.get(5)?,
                prompt_cut: r.get(6)?,
                reply: r.get(7)?,
                reply_cut: r.get(8)?,
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

/// What a presence event carries: the session and its presence, with the
/// attention it asks of the person, which a question or a review waiting
/// on them raises to `needs_you` whatever the agent does.
fn presence_attrs(
    conn: &Connection,
    agent: &str,
    id: &str,
    row: &PresenceRow,
    now: DateTime<Utc>,
) -> rusqlite::Result<Value> {
    let waiting: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM reviews r LEFT JOIN outcomes o ON o.review_id = r.id
           WHERE r.requested_by = ?1 AND r.session = ?2 AND o.review_id IS NULL
             AND (r.expires_at IS NULL OR r.expires_at > ?3))",
        params![agent, id, activity(now)],
        |r| r.get(0),
    )?;
    let attention = crate::presence::attention(Some(row), waiting, now);
    Ok(json!({
        "agent": agent,
        "id": id,
        "presence": crate::presence::to_json(row, attention),
    }))
}

/// The session's presence, as `conn` sees it.
fn read_presence(
    conn: &Connection,
    agent: &str,
    id: &str,
) -> rusqlite::Result<Option<PresenceRow>> {
    let time = |s: String| crate::reviews::parse_datetime(&s).unwrap_or_default();
    conn.query_row(
        "SELECT state, wait_kind, wait_tool, wait_summary, stop_reason, source, pid,
           event_at, state_since, turn, turn_started, finished_at, seen_at, updated_at
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
                event_at: time(r.get(7)?),
                state_since: time(r.get(8)?),
                turn: r.get(9)?,
                turn_started: r.get::<_, Option<String>>(10)?.map(time),
                finished_at: r.get::<_, Option<String>>(11)?.map(time),
                seen_at: r.get::<_, Option<String>>(12)?.map(time),
                updated_at: time(r.get(13)?),
            })
        },
    )
    .optional()
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

/// Keeps the session's turns as the report says, whenever it fired: a
/// prompt opens a turn and ends one still open before it as interrupted,
/// since hooks report no interrupt; the end of a turn closes the latest
/// turn that started before it, even one that already ended, whose end and
/// reply it then replaces; the end of the session closes the open turn as
/// `ended`.
#[allow(clippy::too_many_arguments)]
fn record_turns(
    conn: &Connection,
    agent: &str,
    id: &str,
    report: &Report,
    at: &str,
    now: &str,
    keep_texts: bool,
    late: bool,
) -> rusqlite::Result<Option<(i64, Option<String>)>> {
    let changed = match &report.event {
        ReportEvent::Prompt { prompt } => {
            conn.execute(
                "UPDATE session_turns SET ended_at = ?3, end_kind = 'interrupted'
                 WHERE agent = ?1 AND session = ?2 AND ended_at IS NULL AND started_at <= ?3",
                params![agent, id, at],
            )?;
            conn.execute(
                "INSERT INTO session_turns (agent, session, turn, agent_turn, started_at)
                 VALUES (?1, ?2, (SELECT COALESCE(MAX(turn), 0) + 1 FROM session_turns
                                  WHERE agent = ?1 AND session = ?2), ?4, ?3)",
                params![agent, id, at, report.agent_turn],
            )?;
            let turn_id = conn.last_insert_rowid();
            if keep_texts && prompt.is_some() {
                conn.execute(
                    "INSERT INTO session_turn_texts (turn_id, prompt, prompt_cut) VALUES (?1, ?2, ?3)",
                    params![
                        turn_id,
                        cut(prompt, PROMPT_CHARS),
                        report.cut || longer(prompt, PROMPT_CHARS)
                    ],
                )?;
            }
            late_prompt(conn, agent, id, turn_id, at, late)?;
            let number: i64 = conn.query_row(
                "SELECT turn FROM session_turns WHERE id = ?1",
                [turn_id],
                |r| r.get(0),
            )?;
            // a new turn is the session's activity, and brings it back from
            // the archive
            conn.execute(
                "UPDATE sessions SET last_at = MAX(last_at, ?3), archived_at = NULL
                 WHERE agent = ?1 AND id = ?2",
                params![agent, id, now],
            )?;
            Some((number, None))
        }
        ReportEvent::TurnEnded { end, reply } => {
            // only work that finished continues a turn that already ended:
            // an interrupt, such as Claude Code's idle prompt a minute
            // later, leaves a finished turn as it was
            let continued = end != "interrupted";
            // the agent's own turn id finds its turn exactly; without one,
            // the turn that ran when the end fired
            let named = match &report.agent_turn {
                Some(turn) => conn
                    .query_row(
                        "SELECT id FROM session_turns
                         WHERE agent = ?1 AND session = ?2 AND agent_turn = ?3
                           AND (?4 OR ended_at IS NULL OR end_kind = 'interrupted')",
                        params![agent, id, turn, continued],
                        |r| r.get(0),
                    )
                    .optional()?,
                None => None,
            };
            let turn = match named {
                Some(turn) => Some(turn),
                None => turn_at(conn, agent, id, at, continued)?,
            };
            match turn {
                Some(turn) => {
                    let excerpt = Excerpt {
                        text: cut(reply, REPLY_CHARS),
                        cut: report.cut || longer(reply, REPLY_CHARS),
                    };
                    close(conn, turn, end, at, excerpt, keep_texts)?;
                    Some((number_of(conn, turn)?, Some(end.clone())))
                }
                None => None,
            }
        }
        ReportEvent::Ended { .. } => match turn_at(conn, agent, id, at, false)? {
            Some(turn) => {
                let excerpt = Excerpt {
                    text: None,
                    cut: false,
                };
                close(conn, turn, "ended", at, excerpt, false)?;
                Some((number_of(conn, turn)?, Some("ended".to_string())))
            }
            None => None,
        },
        ReportEvent::Started { .. } | ReportEvent::Working | ReportEvent::NeedsInput { .. } => None,
    };
    Ok(changed)
}

/// The number of a turn, by its row id.
fn number_of(conn: &Connection, turn_id: i64) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT turn FROM session_turns WHERE id = ?1",
        [turn_id],
        |r| r.get(0),
    )
}

/// Puts a prompt that arrived after reports that fired later in its place:
/// an end that arrived before it, and fired after it, is its turn's, and a
/// turn that started after it ends it. The turns are numbered again in the
/// order they started.
///
/// An end that arrived before its prompt had first closed the turn before,
/// replacing that turn's end and reply, which cannot be restored: the turn
/// before is left ending where this one began, with no reply. Claude Code
/// runs its hooks one at a time, so this needs a turn that ends within
/// moments of its prompt and hooks that run apart, as Codex's do.
fn late_prompt(
    conn: &Connection,
    agent: &str,
    id: &str,
    turn_id: i64,
    at: &str,
    late: bool,
) -> rusqlite::Result<()> {
    // a turn before it that a later prompt cut short was cut short here
    conn.execute(
        "UPDATE session_turns SET ended_at = ?4
         WHERE agent = ?1 AND session = ?2 AND id <> ?3 AND started_at < ?4
           AND end_kind = 'interrupted' AND ended_at > ?4",
        params![agent, id, turn_id, at],
    )?;
    // the turn before it, if an end that fired after this prompt closed it.
    // Only a prompt that arrived after a newer report is late: one whose
    // hook clock merely stepped back leaves the turns as they are
    if !late {
        return renumber_after(conn, agent, id, turn_id, at);
    }
    let before: Option<(i64, String, String)> = conn
        .query_row(
            "SELECT id, ended_at, end_kind FROM session_turns
             WHERE agent = ?1 AND session = ?2 AND id <> ?3 AND started_at < ?4
               AND ended_at > ?4 AND end_kind <> 'interrupted'
             ORDER BY started_at DESC LIMIT 1",
            params![agent, id, turn_id, at],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((previous, ended_at, end_kind)) = before {
        conn.execute(
            "UPDATE session_turns SET ended_at = ?2, end_kind = ?3 WHERE id = ?1",
            params![turn_id, ended_at, end_kind],
        )?;
        // its own end is not known any more: it was cut short here
        conn.execute(
            "UPDATE session_turns SET ended_at = ?2, end_kind = 'interrupted' WHERE id = ?1",
            params![previous, at],
        )?;
        // the reply goes with its end
        conn.execute(
            "INSERT INTO session_turn_texts (turn_id, reply, reply_cut)
               SELECT ?2, reply, reply_cut FROM session_turn_texts
               WHERE turn_id = ?1 AND reply IS NOT NULL
             ON CONFLICT (turn_id) DO UPDATE SET reply = excluded.reply,
               reply_cut = excluded.reply_cut",
            params![previous, turn_id],
        )?;
        conn.execute(
            "UPDATE session_turn_texts SET reply = NULL, reply_cut = 0 WHERE turn_id = ?1",
            params![previous],
        )?;
    }
    renumber_after(conn, agent, id, turn_id, at)
}

/// Ends the turn where a later one starts, if one does, and numbers the
/// turns again in the order they started.
fn renumber_after(
    conn: &Connection,
    agent: &str,
    id: &str,
    turn_id: i64,
    at: &str,
) -> rusqlite::Result<()> {
    let next: Option<String> = conn
        .query_row(
            "SELECT MIN(started_at) FROM session_turns
             WHERE agent = ?1 AND session = ?2 AND started_at > ?3",
            params![agent, id, at],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if let Some(next) = next {
        conn.execute(
            "UPDATE session_turns SET ended_at = ?2, end_kind = 'interrupted'
             WHERE id = ?1 AND ended_at IS NULL",
            params![turn_id, next],
        )?;
        conn.execute(
            "UPDATE session_turns SET turn = -turn WHERE agent = ?1 AND session = ?2",
            params![agent, id],
        )?;
        conn.execute(
            "UPDATE session_turns SET turn = (
               SELECT COUNT(*) FROM session_turns t
               WHERE t.agent = session_turns.agent AND t.session = session_turns.session
                 AND (t.started_at < session_turns.started_at
                      OR (t.started_at = session_turns.started_at AND t.id <= session_turns.id)))
             WHERE agent = ?1 AND session = ?2",
            params![agent, id],
        )?;
    }
    Ok(())
}

/// The turn that ran at `at`: the latest that started by then. With
/// `continued`, also one that already ended, since work that went on after
/// its end, such as a subagent's result handed back, belongs to it; without,
/// only one still open, or ended as interrupted by a later prompt.
fn turn_at(
    conn: &Connection,
    agent: &str,
    id: &str,
    at: &str,
    continued: bool,
) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM session_turns
         WHERE agent = ?1 AND session = ?2 AND started_at < ?3
           AND (?4 OR ended_at IS NULL OR (end_kind = 'interrupted' AND ended_at >= ?3))
         ORDER BY started_at DESC, turn DESC LIMIT 1",
        params![agent, id, at, continued],
        |r| r.get(0),
    )
    .optional()
}

/// Ends the turn as `end` at `at`, with the agent's reply when the texts are
/// kept.
fn close(
    conn: &Connection,
    turn_id: i64,
    end: &str,
    at: &str,
    reply: Excerpt,
    keep_texts: bool,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE session_turns SET ended_at = ?2, end_kind = ?3 WHERE id = ?1",
        params![turn_id, at, end],
    )?;
    // the reply is the one of the end that closed the turn, none included
    if keep_texts && reply.text.is_some() {
        conn.execute(
            "INSERT INTO session_turn_texts (turn_id, reply, reply_cut) VALUES (?1, ?2, ?3)
             ON CONFLICT (turn_id) DO UPDATE SET reply = excluded.reply,
               reply_cut = excluded.reply_cut",
            params![turn_id, reply.text, reply.cut],
        )?;
    } else {
        conn.execute(
            "UPDATE session_turn_texts SET reply = NULL, reply_cut = 0 WHERE turn_id = ?1",
            params![turn_id],
        )?;
    }
    Ok(())
}
