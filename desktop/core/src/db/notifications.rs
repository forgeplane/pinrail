//! The person's notifications: one row for each review, work item or
//! session that has something to report, with the reason of its latest
//! event, and whether the person has read it, marked it done or must act
//! on it. The row is deleted with its subject.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::Db;
use super::events::EventSubject;
use crate::reviews::{iso, parse_datetime};

/// A notification as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub id: i64,
    pub subject: EventSubject,
    /// The subject's repository, by name.
    pub repo: Option<String>,
    /// The reason of the latest event, such as `item_commented`.
    pub reason: String,
    /// Whether the person must act on it.
    pub waiting: bool,
    pub last_event: i64,
    /// Who caused the latest event, by name.
    pub last_actor: Option<String>,
    /// A line from the latest event.
    pub summary: Option<String>,
    /// The last event the person has seen.
    pub read_event: i64,
    /// How many events came after the one the person has seen.
    pub unread: i64,
    pub updated_at: DateTime<Utc>,
    pub done_at: Option<DateTime<Utc>>,
    pub muted: bool,
}

/// What an event reports on its subject's notification.
#[derive(Debug, Clone)]
pub struct Notify<'a> {
    pub subject: &'a EventSubject,
    pub reason: &'a str,
    pub waiting: bool,
    pub event: i64,
    pub actor: Option<&'a str>,
    pub summary: Option<&'a str>,
    pub at: DateTime<Utc>,
}

/// The notifications a list shows, newest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listed {
    /// What the person must act on.
    Waiting,
    /// What has events the person has not seen, and is not done.
    Unread,
    /// Everything not done.
    Open,
}

/// How many notifications wait for the person, and how many are unread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub waiting: i64,
    pub unread: i64,
}

/// The longest summary kept.
pub const SUMMARY_MAX: usize = 200;

const COLUMNS: &str = "n.id, n.review_id, n.item_id, n.session_id, g.name, n.reason, n.waiting,
  n.last_event, n.last_actor, n.summary, n.read_event, n.unread, n.updated_at, n.done_at, n.muted";

/// The column that names a subject, and its id.
fn subject_column(subject: &EventSubject) -> (&'static str, &str) {
    match subject {
        EventSubject::Review(id) => ("review_id", id),
        EventSubject::Item(id) => ("item_id", id),
        EventSubject::Session(id) => ("session_id", id),
    }
}

/// Records what an event reports on its subject's notification: creates
/// the row when there is none, takes the event's reason, actor and summary,
/// counts it unread and brings it back when it was done. Returns the
/// notification's id, or `None` when it is muted, which no event changes.
pub(super) fn report(conn: &Connection, report: &Notify<'_>) -> rusqlite::Result<Option<i64>> {
    let (column, id) = subject_column(report.subject);
    let repo = match report.subject {
        EventSubject::Review(_) => "SELECT repo_id FROM reviews WHERE id = ?1",
        EventSubject::Item(_) => "SELECT repo_id FROM work_items WHERE id = ?1",
        EventSubject::Session(_) => "SELECT repo_id FROM sessions WHERE id = ?1",
    };
    let summary = report
        .summary
        .map(|s| s.chars().take(SUMMARY_MAX).collect::<String>());
    conn.query_row(
        &format!(
            "INSERT INTO notifications ({column}, repo_id, reason, waiting, last_event, last_actor,
               summary, unread, updated_at)
             VALUES (?1, ({repo}), ?2, ?3, ?4, ?5, ?6, 1, ?7)
             ON CONFLICT ({column}) WHERE {column} IS NOT NULL DO UPDATE SET
               repo_id = excluded.repo_id, reason = excluded.reason,
               waiting = excluded.waiting, last_event = excluded.last_event,
               last_actor = excluded.last_actor, summary = excluded.summary,
               updated_at = excluded.updated_at,
               -- a done notification comes back with this event alone
               unread = CASE WHEN done_at IS NULL THEN unread + 1 ELSE 1 END,
               done_at = NULL
             WHERE muted = 0
             RETURNING id"
        ),
        params![
            id,
            report.reason,
            report.waiting,
            report.event,
            report.actor,
            summary,
            iso(report.at)
        ],
        |r| r.get(0),
    )
    .optional()
}

impl Db {
    /// Records what an event reports on its subject's notification, as
    /// [`report`] does.
    pub fn report_notification(&self, report: &Notify<'_>) -> rusqlite::Result<Option<i64>> {
        self::report(&self.conn(), report)
    }

    /// The notification of this id.
    pub fn notification(&self, id: i64) -> rusqlite::Result<Option<Notification>> {
        let conn = self.read();
        conn.query_row(
            &format!(
                "SELECT {COLUMNS} FROM notifications n LEFT JOIN repos g ON g.id = n.repo_id
                  WHERE n.id = ?1"
            ),
            params![id],
            row_to_notification,
        )
        .optional()
    }

    /// The notifications of a list, newest first, at most `limit`.
    pub fn notifications(
        &self,
        listed: Listed,
        limit: usize,
    ) -> rusqlite::Result<Vec<Notification>> {
        let filter = match listed {
            Listed::Waiting => "n.waiting = 1",
            Listed::Unread => "n.unread > 0 AND n.done_at IS NULL",
            Listed::Open => "n.done_at IS NULL",
        };
        let conn = self.read();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM notifications n LEFT JOIN repos g ON g.id = n.repo_id
              WHERE {filter} ORDER BY n.last_event DESC LIMIT ?1"
        ))?;
        let rows = stmt.query_map(params![limit as i64], row_to_notification)?;
        rows.collect()
    }

    /// How many notifications wait for the person, and how many are unread.
    pub fn notification_counts(&self) -> rusqlite::Result<Counts> {
        let conn = self.read();
        conn.query_row(
            "SELECT (SELECT count(*) FROM notifications WHERE waiting = 1),
                    (SELECT count(*) FROM notifications WHERE unread > 0 AND done_at IS NULL)",
            [],
            |r| {
                Ok(Counts {
                    waiting: r.get(0)?,
                    unread: r.get(1)?,
                })
            },
        )
    }

    /// Marks a notification read: the person has seen its latest event.
    /// `false` when there is no such notification.
    pub fn mark_notification_read(&self, id: i64) -> rusqlite::Result<bool> {
        let conn = self.conn();
        Ok(conn.execute(
            "UPDATE notifications SET read_event = last_event, unread = 0 WHERE id = ?1",
            params![id],
        )? > 0)
    }

    /// Marks a notification unread, as one event the person has not seen.
    pub fn mark_notification_unread(&self, id: i64) -> rusqlite::Result<bool> {
        let conn = self.conn();
        Ok(conn.execute(
            "UPDATE notifications SET unread = MAX(unread, 1) WHERE id = ?1",
            params![id],
        )? > 0)
    }

    /// Marks a notification done at `at`, which takes it out of the lists
    /// until its subject's next event.
    pub fn mark_notification_done(&self, id: i64, at: DateTime<Utc>) -> rusqlite::Result<bool> {
        let conn = self.conn();
        Ok(conn.execute(
            "UPDATE notifications SET done_at = ?2 WHERE id = ?1",
            params![id, iso(at)],
        )? > 0)
    }
}

fn row_to_notification(row: &Row<'_>) -> rusqlite::Result<Notification> {
    let review: Option<String> = row.get(1)?;
    let item: Option<String> = row.get(2)?;
    let session: Option<String> = row.get(3)?;
    let subject = match (review, item, session) {
        (Some(id), _, _) => EventSubject::Review(id),
        (_, Some(id), _) => EventSubject::Item(id),
        (_, _, Some(id)) => EventSubject::Session(id),
        // the table's check gives every row one subject
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    let updated_at: String = row.get(12)?;
    let done_at: Option<String> = row.get(13)?;
    Ok(Notification {
        id: row.get(0)?,
        subject,
        repo: row.get(4)?,
        reason: row.get(5)?,
        waiting: row.get(6)?,
        last_event: row.get(7)?,
        last_actor: row.get(8)?,
        summary: row.get(9)?,
        read_event: row.get(10)?,
        unread: row.get(11)?,
        updated_at: parse_datetime(&updated_at).unwrap_or_default(),
        done_at: done_at.and_then(|d| parse_datetime(&d)),
        muted: row.get(14)?,
    })
}
