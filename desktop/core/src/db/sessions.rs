//! An agent's sessions, derived from the reviews they sent: a session is
//! the reviews and questions with one agent's name and one session id.

use chrono::{DateTime, Utc};
use rusqlite::params_from_iter;

use super::Db;
use crate::reviews::parse_datetime;

/// One session, summed up from its reviews and questions.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRow {
    pub agent: String,
    pub id: String,
    /// where the session's newest review comes from
    pub repo: Option<String>,
    pub reference: Option<String>,
    pub first_at: DateTime<Utc>,
    pub last_at: DateTime<Utc>,
    pub questions: usize,
    pub reviews: usize,
    pub waiting_questions: usize,
    pub waiting_reviews: usize,
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
    pub limit: usize,
    pub offset: usize,
}

impl Db {
    /// The sessions the filters admit, those waiting for the person first,
    /// then the newest; and how many there are in all.
    pub fn sessions(
        &self,
        filters: &SessionFilters,
        now: DateTime<Utc>,
    ) -> rusqlite::Result<(Vec<SessionRow>, usize)> {
        let mut args: Vec<String> = vec![crate::reviews::iso(now)];
        let mut grouped = String::new();
        if let Some(agent) = &filters.agent {
            args.push(agent.clone());
            grouped.push_str(&format!(" AND r.requested_by = ?{}", args.len()));
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
                    "SUM(r.title LIKE ?{n} ESCAPE '\\' OR r.requested_by LIKE ?{n} ESCAPE '\\' \
                     OR r.session LIKE ?{n} ESCAPE '\\') > 0"
                ));
            }
        }
        let mut outer = Vec::new();
        if let Some(repo) = &filters.repo {
            args.push(repo.clone());
            outer.push(format!("repo = ?{}", args.len()));
        }
        if filters.waiting {
            outer.push("waiting_questions + waiting_reviews > 0".to_string());
        }
        let having = match having.is_empty() {
            true => String::new(),
            false => format!(" HAVING {}", having.join(" AND ")),
        };
        let outer = match outer.is_empty() {
            true => String::new(),
            false => format!(" WHERE {}", outer.join(" AND ")),
        };
        let sessions = format!(
            "WITH s AS ({}{grouped} GROUP BY r.requested_by, r.session{having}) SELECT * FROM s{outer}",
            grouping()
        );
        let conn = self.conn();
        let total: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM ({sessions})"),
            params_from_iter(args.iter()),
            |row| row.get(0),
        )?;
        let mut sql = format!(
            "{sessions} ORDER BY waiting_questions + waiting_reviews > 0 DESC, last_event DESC"
        );
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
            "{} AND r.requested_by = ?2 AND r.session = ?3 GROUP BY r.requested_by, r.session",
            grouping()
        );
        let conn = self.conn();
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query_map(
            rusqlite::params![crate::reviews::iso(now), agent, id],
            row_to_session,
        )?;
        rows.next().transpose()
    }
}

/// The sessions, one row each, before the `GROUP BY`; `?1` is the time now,
/// for what still waits.
fn grouping() -> String {
    let pending = "(o.review_id IS NULL AND (r.expires_at IS NULL OR r.expires_at > ?1))";
    let newest = |field: &str| {
        format!(
            "(SELECT json_extract(x.origin, '$.{field}') FROM reviews x \
             WHERE x.requested_by = r.requested_by AND x.session = r.session \
             ORDER BY x.created_at DESC, x.id DESC LIMIT 1)"
        )
    };
    // the session's newest event, not counting the person opening a review:
    // ids order what happened within one second, which the times do not
    let last = |column: &str| {
        format!(
            "(SELECT e.{column} FROM events e JOIN reviews x ON x.id = e.review_id \
             WHERE x.requested_by = r.requested_by AND x.session = r.session AND e.kind <> 'viewed' \
             ORDER BY e.id DESC LIMIT 1)"
        )
    };
    format!(
        "SELECT r.requested_by AS agent, r.session AS id, {repo} AS repo, {reference} AS reference,
    MIN(r.created_at) AS first_at, {last_at} AS last_at,
    SUM(r.kind = 'question') AS questions, SUM(r.kind = 'review') AS reviews,
    SUM(r.kind = 'question' AND {pending}) AS waiting_questions,
    SUM(r.kind = 'review' AND {pending}) AS waiting_reviews,
    {last_event} AS last_event
  FROM reviews r LEFT JOIN outcomes o ON o.review_id = r.id
  WHERE r.requested_by IS NOT NULL AND r.session IS NOT NULL",
        repo = newest("repo"),
        reference = newest("ref"),
        last_at = last("at"),
        last_event = last("id"),
    )
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    let time = |i: usize| -> rusqlite::Result<DateTime<Utc>> {
        Ok(parse_datetime(&row.get::<_, String>(i)?).unwrap_or_default())
    };
    let count = |i: usize| -> rusqlite::Result<usize> { Ok(row.get::<_, i64>(i)? as usize) };
    Ok(SessionRow {
        agent: row.get(0)?,
        id: row.get(1)?,
        repo: row.get(2)?,
        reference: row.get(3)?,
        first_at: time(4)?,
        last_at: time(5)?,
        questions: count(6)?,
        reviews: count(7)?,
        waiting_questions: count(8)?,
        waiting_reviews: count(9)?,
    })
}
