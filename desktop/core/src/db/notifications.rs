//! The person's notifications: one row for each review, work item or
//! session that has something to report, with the reason of its latest
//! event, and whether the person has read it, marked it done or must act
//! on it. The row is deleted with its subject.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{Value, json};

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

impl Listed {
    /// The condition on `n`, the notification, that keeps the list's rows.
    fn filter(self) -> &'static str {
        match self {
            Listed::Waiting => "n.waiting = 1",
            Listed::Unread => "n.unread > 0 AND n.done_at IS NULL",
            Listed::Open => "n.done_at IS NULL",
        }
    }
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

/// Makes the table of the reasons the inbox does not show, which the
/// person's settings fill. It is the write connection's own, as the
/// projector runs there alone.
pub(super) fn prepare(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("CREATE TEMP TABLE IF NOT EXISTS inbox_hidden (reason TEXT PRIMARY KEY)")
}

/// Whether the person turned off the reason, so that it adds nothing to
/// the inbox.
fn hidden(conn: &Connection, reason: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM temp.inbox_hidden WHERE reason = ?1)",
        params![reason],
        |r| r.get(0),
    )
}

/// An event as the projector reads it.
#[derive(Debug, Clone)]
pub(super) struct Projected<'a> {
    pub event: i64,
    pub kind: &'a str,
    pub subject: Option<&'a EventSubject>,
    /// Whether the person caused it, who needs no notice of it.
    pub by_person: bool,
    pub actor: Option<&'a str>,
    pub attrs: &'a Value,
    pub at: DateTime<Utc>,
}

/// Applies an event to the notification of its subject, in the event's
/// transaction. An agent's or Pinrail's change to a work item reports on
/// the item, or for a new sub-item on its parent, so that a breakdown makes
/// one notification. The person's own change marks the item's
/// notification read.
pub(super) fn project(conn: &Connection, event: &Projected<'_>) -> rusqlite::Result<()> {
    if let Some(EventSubject::Review(review)) = event.subject {
        return project_review(conn, review, event);
    }
    let Some(EventSubject::Item(item)) = event.subject else {
        return Ok(());
    };
    if event.kind != crate::events::ITEM_CHANGED {
        return Ok(());
    }
    if event.by_person {
        conn.execute(
            "UPDATE notifications SET read_event = last_event, unread = 0 WHERE item_id = ?1",
            params![item],
        )?;
        return Ok(());
    }
    let attr = |name: &str| event.attrs[name].as_str();
    let (reason, subject, summary) = match attr("change") {
        Some("created") => {
            let parent: Option<String> = conn.query_row(
                "SELECT parent FROM work_items WHERE id = ?1",
                params![item],
                |r| r.get(0),
            )?;
            let title: String = conn.query_row(
                "SELECT title FROM work_items WHERE id = ?1",
                params![item],
                |r| r.get(0),
            )?;
            let summary = format!("{} {title}", attr("key").unwrap_or_default());
            match parent {
                Some(parent) => ("item_added", EventSubject::Item(parent), Some(summary)),
                None => ("item_added", EventSubject::Item(item.clone()), None),
            }
        }
        Some("commented") => (
            "item_commented",
            EventSubject::Item(item.clone()),
            attr("excerpt").map(str::to_string),
        ),
        Some("status") if matches!(attr("to"), Some("done" | "cancelled")) => {
            ("item_closed", EventSubject::Item(item.clone()), None)
        }
        _ => ("item_changed", EventSubject::Item(item.clone()), None),
    };
    if hidden(conn, reason)? {
        return Ok(());
    }
    report(
        conn,
        &Notify {
            subject: &subject,
            reason,
            waiting: false,
            event: event.event,
            actor: event.actor,
            summary: summary.as_deref(),
            at: event.at,
        },
    )?;
    Ok(())
}

/// Applies an event about a review or a question to its notification. A
/// new one waits for the person, and a new round takes over its earlier
/// round's notification. Opening it marks it read; deciding, answering or
/// discarding it ends the wait and marks it done. A withdrawal or an expiry
/// ends the wait and says so, unless the person turned that reason off.
fn project_review(conn: &Connection, review: &str, event: &Projected<'_>) -> rusqlite::Result<()> {
    use crate::events as e;
    let read = "read_event = last_event, unread = 0";
    let update = |set: &str| {
        conn.execute(
            &format!("UPDATE notifications SET {set} WHERE review_id = ?1"),
            params![review],
        )
    };
    match event.kind {
        e::CREATED => {
            let (kind, revises): (String, Option<String>) = conn.query_row(
                "SELECT kind, revises FROM reviews WHERE id = ?1",
                params![review],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            // the earlier round's notification moves to the new round
            if let Some(revises) = &revises {
                conn.execute(
                    "UPDATE notifications SET review_id = ?1 WHERE review_id = ?2",
                    params![review, revises],
                )?;
            }
            let reason = match (kind.as_str(), &revises) {
                ("question", _) => "question_asked",
                (_, Some(_)) => "new_round",
                _ => "review_requested",
            };
            report(
                conn,
                &Notify {
                    subject: &EventSubject::Review(review.to_string()),
                    reason,
                    waiting: true,
                    event: event.event,
                    actor: event.actor,
                    summary: None,
                    at: event.at,
                },
            )?;
        }
        e::DECIDED | e::DISCARDED => {
            conn.execute(
                &format!(
                    "UPDATE notifications SET waiting = 0, {read}, done_at = ?2 WHERE review_id = ?1"
                ),
                params![review, iso(event.at)],
            )?;
        }
        e::WITHDRAWN | e::EXPIRED => {
            // a round superseded by a newer one gave its notification to
            // that round: nothing is left to tell
            let notified: bool = conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM notifications WHERE review_id = ?1)",
                params![review],
                |r| r.get(0),
            )?;
            if !notified {
                return Ok(());
            }
            let reason = if event.kind == e::WITHDRAWN {
                "review_withdrawn"
            } else {
                "review_expired"
            };
            if hidden(conn, reason)? {
                conn.execute(
                    "UPDATE notifications SET waiting = 0, done_at = ?2 WHERE review_id = ?1",
                    params![review, iso(event.at)],
                )?;
            } else {
                report(
                    conn,
                    &Notify {
                        subject: &EventSubject::Review(review.to_string()),
                        reason,
                        waiting: false,
                        event: event.event,
                        actor: event.actor,
                        summary: None,
                        at: event.at,
                    },
                )?;
            }
        }
        // the person opening it, or anything else they do to it
        _ if event.by_person || event.kind == e::VIEWED => {
            update(read)?;
        }
        _ => {}
    }
    Ok(())
}

impl Counts {
    pub fn to_json(&self) -> Value {
        json!({"waiting": self.waiting, "unread": self.unread})
    }
}

impl Db {
    /// Sets the reasons the inbox does not show, from the person's
    /// settings.
    pub fn set_inbox_hidden(&self, reasons: &[&str]) -> rusqlite::Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM temp.inbox_hidden", [])?;
        for reason in reasons {
            tx.execute(
                "INSERT INTO temp.inbox_hidden (reason) VALUES (?1)",
                params![reason],
            )?;
        }
        tx.commit()
    }

    /// The notification an event changed: the one it reported on, or the
    /// notification of its subject.
    pub fn notification_of(
        &self,
        event: i64,
        subject: Option<&EventSubject>,
    ) -> rusqlite::Result<Option<Notification>> {
        let conn = self.read();
        let by_event = conn
            .query_row(
                &format!(
                    "SELECT {COLUMNS} FROM notifications n LEFT JOIN repos g ON g.id = n.repo_id
                      WHERE n.last_event = ?1"
                ),
                params![event],
                row_to_notification,
            )
            .optional()?;
        if by_event.is_some() {
            return Ok(by_event);
        }
        let Some(subject) = subject else {
            return Ok(None);
        };
        let (column, id) = subject_column(subject);
        conn.query_row(
            &format!(
                "SELECT {COLUMNS} FROM notifications n LEFT JOIN repos g ON g.id = n.repo_id
                  WHERE n.{column} = ?1"
            ),
            params![id],
            row_to_notification,
        )
        .optional()
    }

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
        Ok(self.notifications_page(listed, limit, 0)?.0)
    }

    /// A page of a list, newest first: at most `limit` from `offset`, and
    /// how many the whole list holds.
    pub fn notifications_page(
        &self,
        listed: Listed,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<(Vec<Notification>, i64)> {
        let filter = listed.filter();
        let conn = self.read();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM notifications n LEFT JOIN repos g ON g.id = n.repo_id
              WHERE {filter} ORDER BY n.last_event DESC LIMIT ?1 OFFSET ?2"
        ))?;
        let rows = stmt
            .query_map(params![limit as i64, offset as i64], row_to_notification)?
            .collect::<rusqlite::Result<_>>()?;
        let total = conn.query_row(
            &format!("SELECT count(*) FROM notifications n WHERE {filter}"),
            [],
            |r| r.get(0),
        )?;
        Ok((rows, total))
    }

    /// Marks read every notification of a list whose latest event is
    /// `up_to` or older: what the person saw listed. Returns how many.
    pub fn read_notifications_up_to(&self, listed: Listed, up_to: i64) -> rusqlite::Result<usize> {
        let conn = self.conn();
        conn.execute(
            &format!(
                "UPDATE notifications AS n SET read_event = last_event, unread = 0
                  WHERE {} AND n.unread > 0 AND n.last_event <= ?1",
                listed.filter()
            ),
            params![up_to],
        )
    }

    /// Takes a notification out of done. `false` when there is none.
    pub fn mark_notification_undone(&self, id: i64) -> rusqlite::Result<bool> {
        let conn = self.conn();
        Ok(conn.execute(
            "UPDATE notifications SET done_at = NULL WHERE id = ?1",
            params![id],
        )? > 0)
    }

    /// The id of the newest event recorded, or 0.
    pub fn newest_event_id(&self) -> rusqlite::Result<i64> {
        let conn = self.read();
        conn.query_row(
            "SELECT COALESCE((SELECT seq FROM sqlite_sequence WHERE name = 'events'), 0)",
            [],
            |r| r.get(0),
        )
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
