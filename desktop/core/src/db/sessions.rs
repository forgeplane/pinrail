//! An agent's sessions: a row for each agent and session id that sent a
//! review or a status, kept in step with the reviews that name it, and the
//! statuses the agent reported, oldest first.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde_json::{Map, Value, json};

use super::Db;
use super::events::insert_event;
use crate::attachments::ReviewAttachment;
use crate::reviews::parse_datetime;

/// One session, with what its questions and reviews hold and wait for.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRow {
    pub agent: String,
    pub id: String,
    /// the project, the branch and the folder the agent last reported
    pub repo: Option<String>,
    pub reference: Option<String>,
    pub folder: Option<String>,
    pub first_at: DateTime<Utc>,
    pub last_at: DateTime<Utc>,
    pub questions: usize,
    pub reviews: usize,
    pub waiting_questions: usize,
    pub waiting_reviews: usize,
    /// what the agent last said it is doing
    pub status: Option<StatusRow>,
    /// the stage the agent last said it started, which names the session
    pub title: Option<String>,
    /// when the person archived the session, if they did
    pub archived_at: Option<DateTime<Utc>>,
    /// when the agent last reported anything, a note included
    pub reported_at: Option<DateTime<Utc>>,
}

/// What a status says: a step of the agent's work, the start of a task,
/// which names it, the end of the agent's work, or a note, a Markdown
/// message that only informs the person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Step,
    Start,
    Done,
    Note,
}

impl StatusKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StatusKind::Step => "step",
            StatusKind::Start => "start",
            StatusKind::Done => "done",
            StatusKind::Note => "note",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "step" => Some(StatusKind::Step),
            "start" => Some(StatusKind::Start),
            "done" => Some(StatusKind::Done),
            "note" => Some(StatusKind::Note),
            _ => None,
        }
    }
}

/// What an agent said it is doing, with `pinrail status`.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusRow {
    pub id: i64,
    pub text: String,
    pub kind: StatusKind,
    pub at: DateTime<Utc>,
    /// in a session's thread, the review the status came after, which the
    /// times, in whole seconds, do not always tell
    pub after: Option<String>,
    /// the files a note carries, by name
    pub attachments: Vec<ReviewAttachment>,
}

impl StatusRow {
    pub fn to_json(&self) -> Value {
        let mut json = json!({
            "id": self.id,
            "text": self.text,
            "kind": self.kind.as_str(),
            "at": crate::reviews::iso(self.at),
            "after": self.after,
        });
        if self.kind == StatusKind::Note {
            json["attachments"] = self
                .attachments
                .iter()
                .map(|a| json!({ "name": a.name, "size": a.size, "media_type": a.media_type }))
                .collect();
        }
        json
    }
}

/// The session and its new status, as a `session_status` notice carries
/// them. A note's Markdown is left out: the thread reads it.
pub fn status_notice(agent: &str, id: &str, status: &StatusRow) -> Value {
    let mut status = status.to_json();
    if status["kind"] == "note" {
        status["text"] = json!("");
    }
    json!({ "agent": agent, "id": id, "status": status })
}

/// A stage an agent started, as a search finds it: the start status and
/// its session.
#[derive(Debug, Clone)]
pub struct StageRow {
    pub id: i64,
    pub agent: String,
    pub session: String,
    pub text: String,
    pub at: DateTime<Utc>,
    pub repo: Option<String>,
}

impl StageRow {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "agent": self.agent,
            "session": self.session,
            "text": self.text,
            "at": crate::reviews::iso(self.at),
            "repo": self.repo,
        })
    }
}

/// Which sessions a listing gives.
#[derive(Debug, Clone, Default)]
pub struct SessionFilters {
    pub agent: Option<String>,
    pub repo: Option<String>,
    /// words found in the agent, the session id or a title in the session
    pub text: Option<String>,
    /// only sessions with something waiting for the person
    pub waiting: bool,
    /// the archived sessions too, which a listing leaves out otherwise
    pub include_archived: bool,
    pub limit: usize,
    pub offset: usize,
}

/// A session's latest activity, to the microsecond, which orders the
/// sessions where the times of reviews, in whole seconds, would not, and
/// where two requests in a row can share a millisecond. The migration wrote
/// the earlier times to the millisecond; as text, they sort with these.
pub(super) fn activity(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

/// Creates the session, or records what it reports now: the project, the
/// branch and the folder it gives, and its activity at `at`.
pub(super) fn touch(
    conn: &Connection,
    agent: &str,
    id: &str,
    origin: &Map<String, Value>,
    at: DateTime<Utc>,
) -> rusqlite::Result<()> {
    let field = |key: &str| origin.get(key).and_then(Value::as_str);
    conn.execute(
        "INSERT INTO sessions (agent, id, repo, ref, folder, first_at, last_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (agent, id) DO UPDATE SET
           repo = COALESCE(excluded.repo, repo),
           ref = COALESCE(excluded.ref, ref),
           folder = COALESCE(excluded.folder, folder),
           last_at = MAX(last_at, excluded.last_at),
           archived_at = NULL",
        params![
            agent,
            id,
            field("repo"),
            field("ref"),
            field("folder"),
            crate::reviews::iso(at),
            activity(at),
        ],
    )?;
    Ok(())
}

/// Records activity in the session of the review, if it names one.
pub(super) fn touch_review(conn: &Connection, review_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sessions SET last_at = MAX(last_at, ?1)
         WHERE (agent, id) = (SELECT requested_by, session FROM reviews WHERE id = ?2)",
        params![activity(Utc::now()), review_id],
    )?;
    Ok(())
}

impl Db {
    /// Archives the session at `at`, keeping an earlier time, or brings it
    /// back with `None`, and records the event: its id, or `None` for a
    /// session that does not exist.
    pub fn set_archived(
        &self,
        agent: &str,
        id: &str,
        at: Option<DateTime<Utc>>,
    ) -> rusqlite::Result<Option<i64>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE sessions SET archived_at = CASE WHEN ?3 IS NULL THEN NULL
               ELSE COALESCE(archived_at, ?3) END
             WHERE agent = ?1 AND id = ?2",
            params![agent, id, at.map(crate::reviews::iso)],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        let kind = if at.is_some() {
            crate::events::SESSION_ARCHIVED
        } else {
            crate::events::SESSION_UNARCHIVED
        };
        let event_id = insert_event(&tx, None, kind, None, &json!({ "agent": agent, "id": id }))?;
        tx.commit()?;
        Ok(Some(event_id))
    }

    /// The projects of all the sessions, sorted, and whether a session has
    /// none: what the project filter offers, whatever it filters by.
    pub fn session_projects(&self) -> rusqlite::Result<(Vec<String>, bool)> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT repo FROM sessions WHERE COALESCE(repo, '') <> '' ORDER BY 1",
        )?;
        let repos = stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        let unassigned = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sessions WHERE COALESCE(repo, '') = '')",
            [],
            |row| row.get(0),
        )?;
        Ok((repos, unassigned))
    }

    /// The sessions the filters admit, those waiting for the person first,
    /// then the newest; and how many there are in all.
    pub fn sessions(
        &self,
        filters: &SessionFilters,
        now: DateTime<Utc>,
    ) -> rusqlite::Result<(Vec<SessionRow>, usize)> {
        let mut args: Vec<String> = vec![crate::reviews::iso(now)];
        let mut wheres = Vec::new();
        if let Some(agent) = &filters.agent {
            args.push(agent.clone());
            wheres.push(format!("s.agent = ?{}", args.len()));
        }
        match filters.repo.as_deref() {
            Some(super::NO_PROJECT) => wheres.push("COALESCE(s.repo, '') = ''".to_string()),
            Some(repo) => {
                args.push(repo.to_string());
                wheres.push(format!("s.repo = ?{}", args.len()));
            }
            None => {}
        }
        if !filters.include_archived {
            wheres.push("s.archived_at IS NULL".to_string());
        }
        let mut having = Vec::new();
        if let Some(text) = &filters.text {
            for word in text.split_whitespace() {
                let escaped = word
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_");
                args.push(format!("%{escaped}%"));
                let n = args.len();
                having.push(format!(
                    "(COALESCE(SUM(r.title LIKE ?{n} ESCAPE '\\'), 0) > 0 \
                     OR s.agent LIKE ?{n} ESCAPE '\\' OR s.id LIKE ?{n} ESCAPE '\\' \
                     OR {title} LIKE ?{n} ESCAPE '\\')",
                    title = title()
                ));
            }
        }
        if filters.waiting {
            having.push(format!("{WAITING} > 0"));
        }
        let wheres = match wheres.is_empty() {
            true => String::new(),
            false => format!(" WHERE {}", wheres.join(" AND ")),
        };
        let having = match having.is_empty() {
            true => String::new(),
            false => format!(" HAVING {}", having.join(" AND ")),
        };
        let sessions = format!("{}{wheres} GROUP BY s.agent, s.id{having}", select());
        let conn = self.conn();
        let total: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM ({sessions})"),
            params_from_iter(args.iter()),
            |row| row.get(0),
        )?;
        let mut sql = format!("{sessions} ORDER BY {WAITING} > 0 DESC, s.last_at DESC");
        if filters.limit > 0 {
            sql.push_str(&format!(
                " LIMIT {} OFFSET {}",
                filters.limit, filters.offset
            ));
        }
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), row_to_session)?;
        Ok((rows.collect::<Result<_, _>>()?, total as usize))
    }

    /// One session, when anything was sent from it.
    pub fn session(
        &self,
        agent: &str,
        id: &str,
        now: DateTime<Utc>,
    ) -> rusqlite::Result<Option<SessionRow>> {
        let sql = format!(
            "{} WHERE s.agent = ?2 AND s.id = ?3 GROUP BY s.agent, s.id",
            select()
        );
        let conn = self.conn();
        conn.query_row(
            &sql,
            params![crate::reviews::iso(now), agent, id],
            row_to_session,
        )
        .optional()
    }

    /// Records what the agent says it is doing, creating its session if it
    /// sent nothing before, with the files a note carries and the event that
    /// announces it. Returns the status and the event's id, or the name of a
    /// file whose blob is gone.
    pub fn add_status(
        &self,
        agent: &str,
        id: &str,
        text: &str,
        kind: StatusKind,
        origin: &Map<String, Value>,
        attachments: &[ReviewAttachment],
    ) -> rusqlite::Result<Result<(StatusRow, i64), String>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let now = Utc::now();
        touch(&tx, agent, id, origin, now)?;
        tx.execute(
            "INSERT INTO session_statuses (agent, session, text, kind, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![agent, id, text, kind.as_str(), crate::reviews::iso(now)],
        )?;
        let status_id = tx.last_insert_rowid();
        for attachment in attachments {
            let inserted = tx.execute(
                "INSERT INTO status_attachments (status_id, name, sha256, size, media_type)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    status_id,
                    attachment.name,
                    attachment.sha256,
                    attachment.size as i64,
                    attachment.media_type
                ],
            );
            // the only foreign key a new note's files can miss is the stored
            // file itself, swept since it was checked
            match inserted {
                Err(rusqlite::Error::SqliteFailure(error, _))
                    if error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY =>
                {
                    return Ok(Err(attachment.name.clone()));
                }
                inserted => {
                    inserted?;
                }
            }
        }
        let status = StatusRow {
            id: status_id,
            text: text.to_string(),
            kind,
            at: parse_datetime(&crate::reviews::iso(now)).unwrap_or(now),
            after: None,
            attachments: attachments.to_vec(),
        };
        let event_id = insert_event(
            &tx,
            None,
            crate::events::SESSION_STATUS,
            None,
            &status_notice(agent, id, &status),
        )?;
        tx.commit()?;
        Ok(Ok((status, event_id)))
    }

    /// The stages agents started in any session whose names hold every
    /// word of `text`, newest first, at most `limit` of them.
    pub fn stages(&self, text: Option<&str>, limit: usize) -> rusqlite::Result<Vec<StageRow>> {
        let mut args: Vec<String> = Vec::new();
        let mut sql = String::from(
            "SELECT st.id, st.agent, st.session, st.text, st.created_at, s.repo
             FROM session_statuses st
             JOIN sessions s ON s.agent = st.agent AND s.id = st.session
             WHERE st.kind = 'start'",
        );
        for word in text.unwrap_or_default().split_whitespace() {
            let escaped = word
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            args.push(format!("%{escaped}%"));
            sql.push_str(&format!(" AND st.text LIKE ?{} ESCAPE '\\'", args.len()));
        }
        sql.push_str(&format!(" ORDER BY st.id DESC LIMIT {limit}"));
        let conn = self.conn();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), |row| {
            Ok(StageRow {
                id: row.get(0)?,
                agent: row.get(1)?,
                session: row.get(2)?,
                text: row.get(3)?,
                at: crate::reviews::parse_datetime(&row.get::<_, String>(4)?).unwrap_or_default(),
                repo: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// The session's statuses, oldest first, each with the review it came
    /// after: the events' ids give the order in which a status and the
    /// session's reviews were recorded.
    pub fn statuses(&self, agent: &str, id: &str) -> rusqlite::Result<Vec<StatusRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "WITH announced AS (
               SELECT e.id AS event, json_extract(e.attrs, '$.status.id') AS status FROM events e
               WHERE e.review_id IS NULL AND e.kind = ?3
                 AND json_extract(e.attrs, '$.agent') = ?1 AND json_extract(e.attrs, '$.id') = ?2
             ),
             created AS (
               SELECT r.id AS review, e.id AS event FROM reviews r
               JOIN events e ON e.review_id = r.id AND e.kind = ?4
               WHERE r.requested_by = ?1 AND r.session = ?2
             )
             SELECT t.id, t.text, t.kind, t.created_at,
               (SELECT c.review FROM created c WHERE c.event < a.event ORDER BY c.event DESC LIMIT 1)
             FROM session_statuses t LEFT JOIN announced a ON a.status = t.id
             WHERE t.agent = ?1 AND t.session = ?2 ORDER BY t.id",
        )?;
        let rows = stmt.query_map(
            params![
                agent,
                id,
                crate::events::SESSION_STATUS,
                crate::events::CREATED
            ],
            |row| {
                Ok(StatusRow {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    kind: kind_of(row.get::<_, String>(2)?),
                    at: parse_datetime(&row.get::<_, String>(3)?).unwrap_or_default(),
                    after: row.get(4)?,
                    attachments: Vec::new(),
                })
            },
        )?;
        let mut statuses: Vec<StatusRow> = rows.collect::<rusqlite::Result<_>>()?;
        // the files of the session's notes, by name
        let mut stmt = conn.prepare(
            "SELECT a.status_id, a.name, a.sha256, a.size, a.media_type FROM status_attachments a
             JOIN session_statuses t ON t.id = a.status_id
             WHERE t.agent = ?1 AND t.session = ?2 ORDER BY a.status_id, a.name",
        )?;
        let files = stmt.query_map(params![agent, id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                ReviewAttachment {
                    name: row.get(1)?,
                    sha256: row.get(2)?,
                    size: row.get::<_, i64>(3)? as u64,
                    media_type: row.get(4)?,
                },
            ))
        })?;
        for file in files {
            let (status_id, attachment) = file?;
            if let Some(status) = statuses.iter_mut().find(|s| s.id == status_id) {
                status.attachments.push(attachment);
            }
        }
        Ok(statuses)
    }

    /// Deletes the sessions with no review left and no activity since
    /// `before`, with their statuses. Returns how many went.
    pub fn delete_idle_sessions(&self, before: DateTime<Utc>) -> rusqlite::Result<usize> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM sessions WHERE last_at < ?1 AND NOT EXISTS
               (SELECT 1 FROM reviews r WHERE r.requested_by = sessions.agent AND r.session = sessions.id)",
            params![activity(before)],
        )
    }
}

/// How many of the session's questions and reviews wait for the person.
const WAITING: &str = "(waiting_questions + waiting_reviews)";

/// The sessions with what their reviews hold, before the `WHERE` and the
/// `GROUP BY`; `?1` is the time now, for what still waits.
fn select() -> String {
    let pending = "(r.id IS NOT NULL AND o.review_id IS NULL \
                   AND (r.expires_at IS NULL OR r.expires_at > ?1))";
    // the latest status is the newest that is not a note
    let status = |column: &str| {
        format!(
            "(SELECT t.{column} FROM session_statuses t \
             WHERE t.agent = s.agent AND t.session = s.id AND t.kind <> 'note' \
             ORDER BY t.id DESC LIMIT 1)"
        )
    };
    format!(
        "SELECT s.agent, s.id, s.repo, s.ref, s.folder, s.first_at, s.last_at,
    COALESCE(SUM(r.kind = 'question'), 0) AS questions,
    COALESCE(SUM(r.kind = 'review'), 0) AS reviews,
    COALESCE(SUM(r.kind = 'question' AND {pending}), 0) AS waiting_questions,
    COALESCE(SUM(r.kind = 'review' AND {pending}), 0) AS waiting_reviews,
    {id} AS status_id, {text} AS status_text, {kind} AS status_kind, {at} AS status_at,
    {title} AS title,
    s.archived_at,
    (SELECT MAX(t.created_at) FROM session_statuses t
     WHERE t.agent = s.agent AND t.session = s.id) AS reported_at
  FROM sessions s
  LEFT JOIN reviews r ON r.requested_by = s.agent AND r.session = s.id
  LEFT JOIN outcomes o ON o.review_id = r.id",
        id = status("id"),
        text = status("text"),
        kind = status("kind"),
        at = status("created_at"),
        title = title(),
    )
}

/// The session's title: the task the agent last said it started.
fn title() -> &'static str {
    "(SELECT t.text FROM session_statuses t \
     WHERE t.agent = s.agent AND t.session = s.id AND t.kind = 'start' ORDER BY t.id DESC LIMIT 1)"
}

/// A stored kind, which the core checks before it writes one.
fn kind_of(text: String) -> StatusKind {
    StatusKind::parse(&text).unwrap_or(StatusKind::Step)
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    let time = |i: usize| -> rusqlite::Result<DateTime<Utc>> {
        Ok(parse_datetime(&row.get::<_, String>(i)?).unwrap_or_default())
    };
    let count = |i: usize| -> rusqlite::Result<usize> { Ok(row.get::<_, i64>(i)? as usize) };
    let status = match row.get::<_, Option<i64>>(11)? {
        Some(id) => Some(StatusRow {
            id,
            text: row.get(12)?,
            kind: kind_of(row.get::<_, String>(13)?),
            at: time(14)?,
            after: None,
            attachments: Vec::new(),
        }),
        None => None,
    };
    Ok(SessionRow {
        agent: row.get(0)?,
        id: row.get(1)?,
        repo: row.get(2)?,
        reference: row.get(3)?,
        folder: row.get(4)?,
        first_at: time(5)?,
        last_at: time(6)?,
        questions: count(7)?,
        reviews: count(8)?,
        waiting_questions: count(9)?,
        waiting_reviews: count(10)?,
        status,
        title: row.get(15)?,
        archived_at: row
            .get::<_, Option<String>>(16)?
            .and_then(|s| parse_datetime(&s)),
        reported_at: row
            .get::<_, Option<String>>(17)?
            .and_then(|s| parse_datetime(&s)),
    })
}
