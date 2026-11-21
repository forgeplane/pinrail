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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Listed {
    /// What the person must act on.
    Waiting,
    /// What has events the person has not seen, and is not done.
    Unread,
    /// Everything not done.
    #[default]
    Open,
    /// What does not wait and is not done: the inbox's Updates.
    Updates,
}

/// What a list of notifications keeps: a tab, and filters whose values
/// are alternatives, combined with each other. An empty filter keeps all.
#[derive(Debug, Clone, Default)]
pub struct NotificationFilters {
    pub listed: Listed,
    /// reasons, such as `item_commented`, or subject kinds: `review`,
    /// `item`, `session`
    pub kinds: Vec<String>,
    /// repositories by name, `-` for none, or `project:<id>`
    pub scopes: Vec<String>,
    /// the plugins of reviews, by name
    pub plugins: Vec<String>,
    /// words in the subject's title, key or name, or in the summary
    pub q: Option<String>,
    /// the notifications marked done too
    pub done: bool,
}

/// The subject kinds a `kind` filter takes beside the reasons.
pub const SUBJECT_KINDS: [&str; 3] = ["review", "item", "session"];

impl NotificationFilters {
    /// The condition on `n`, the notification, and its values in order.
    fn sql(&self) -> (String, Vec<String>) {
        let mut args: Vec<String> = Vec::new();
        let arg = |value: &str, args: &mut Vec<String>| {
            args.push(value.to_string());
            format!("?{}", args.len())
        };
        let mut wheres = vec![match (self.listed, self.done) {
            (Listed::Open, true) => "1 = 1".to_string(),
            (Listed::Updates, true) => "n.waiting = 0".to_string(),
            (listed, _) => listed.filter().to_string(),
        }];
        if !self.kinds.is_empty() {
            let mut any = Vec::new();
            for kind in &self.kinds {
                match kind.as_str() {
                    "review" => any.push("n.review_id IS NOT NULL".to_string()),
                    "item" => any.push("n.item_id IS NOT NULL".to_string()),
                    "session" => any.push("n.session_id IS NOT NULL".to_string()),
                    reason => any.push(format!("n.reason = {}", arg(reason, &mut args))),
                }
            }
            wheres.push(format!("({})", any.join(" OR ")));
        }
        if !self.scopes.is_empty() {
            let mut any = Vec::new();
            for scope in &self.scopes {
                any.push(match scope.strip_prefix("project:") {
                    _ if scope == "-" => "n.repo_id IS NULL".to_string(),
                    Some(project) => super::projects::in_project("n.repo_id", {
                        arg(project, &mut args);
                        args.len()
                    }),
                    None => super::repos::named("n.repo_id", {
                        arg(scope, &mut args);
                        args.len()
                    }),
                });
            }
            wheres.push(format!("({})", any.join(" OR ")));
        }
        if !self.plugins.is_empty() {
            let list: Vec<String> = self.plugins.iter().map(|p| arg(p, &mut args)).collect();
            wheres.push(format!(
                "n.review_id IN (SELECT id FROM reviews WHERE plugin IN ({}))",
                list.join(", ")
            ));
        }
        if let Some(q) = self.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            let escaped = q
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            let like = arg(&format!("%{escaped}%"), &mut args);
            wheres.push(format!(
                "(n.summary LIKE {like} ESCAPE '\\'
                  OR EXISTS (SELECT 1 FROM reviews r WHERE r.id = n.review_id
                               AND r.title LIKE {like} ESCAPE '\\')
                  OR EXISTS (SELECT 1 FROM work_items w JOIN repos k ON k.id = w.repo_id
                              WHERE w.id = n.item_id
                                AND (w.title LIKE {like} ESCAPE '\\'
                                     OR k.prefix || '-' || w.number LIKE {like} ESCAPE '\\'))
                  OR EXISTS (SELECT 1 FROM sessions s WHERE s.id = n.session_id
                               AND s.name LIKE {like} ESCAPE '\\'))"
            ));
        }
        (wheres.join(" AND "), args)
    }
}

impl Listed {
    /// The condition on `n`, the notification, that keeps the list's rows.
    fn filter(self) -> &'static str {
        match self {
            Listed::Waiting => "n.waiting = 1",
            Listed::Unread => "n.unread > 0 AND n.done_at IS NULL",
            Listed::Open => "n.done_at IS NULL",
            Listed::Updates => "n.waiting = 0 AND n.done_at IS NULL",
        }
    }
}

/// What is new on a notification's subject since the person last looked,
/// for its row's line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct New {
    /// sub-items added, and the latest one's key and title
    pub sub_items: i64,
    pub sub_item: Option<(String, String)>,
    /// whether the work item itself is new, with its sub-items: a breakdown
    pub breakdown: bool,
    /// comments, their authors in the order they wrote, and the latest one's
    /// start
    pub comments: i64,
    pub authors: Vec<String>,
    pub comment: Option<String>,
    /// any other changes, such as a status or a claim
    pub changes: i64,
    /// which of the three came last: `sub_item`, `comment` or `change`
    pub latest: Option<&'static str>,
    /// the work item's parent, by key
    pub parent: Option<String>,
    /// for a session: the turns finished since it was seen, and the first
    /// line of the latest one's reply
    pub turns: i64,
    pub reply: Option<String>,
}

impl New {
    pub fn to_json(&self) -> Value {
        json!({
            "sub_items": self.sub_items,
            "sub_item": self.sub_item.as_ref().map(|(key, title)| json!({"key": key, "title": title})),
            "breakdown": self.breakdown,
            "comments": self.comments,
            "authors": self.authors,
            "comment": self.comment,
            "changes": self.changes,
            "latest": self.latest,
            "parent": self.parent,
            "turns": self.turns,
            "reply": self.reply,
        })
    }
}

/// An event of a work item: its id, its actor, what changed, and a
/// comment's start.
type OwnEvent = (i64, Option<String>, Option<String>, Option<String>);

/// The first line of a text that says something, at most 200 characters,
/// without a Markdown heading's marks.
fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .map(|l| l.chars().take(SUMMARY_MAX).collect())
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
    if let Some(EventSubject::Session(session)) = event.subject {
        return project_session(conn, session, event);
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
        Some("claimed") => ("item_claimed", EventSubject::Item(item.clone()), None),
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

/// Applies an event about a session to its notification. An agent that
/// waits for a permission or an answer in its terminal waits on the
/// person until it moves on; a finished or failed turn, an agent that
/// stopped or whose run failed, and a note are reported. The person seeing
/// the session marks it read, and marking it unread marks it unread. The
/// presence reports of an agent at work change nothing else.
fn project_session(
    conn: &Connection,
    session: &str,
    event: &Projected<'_>,
) -> rusqlite::Result<()> {
    use crate::events as e;
    let attrs = event.attrs;
    let set = |set: &str| {
        conn.execute(
            &format!("UPDATE notifications SET {set} WHERE session_id = ?1"),
            params![session],
        )
    };
    if event.kind == e::SESSION_SEEN {
        let seen = !attrs["presence"]["seen_at"].is_null();
        set(if seen {
            "read_event = last_event, unread = 0"
        } else {
            "unread = MAX(unread, 1)"
        })?;
        return Ok(());
    }
    if event.by_person {
        set("read_event = last_event, unread = 0")?;
        return Ok(());
    }
    let text = |value: &Value| value.as_str().map(str::to_string);
    let (reason, waiting, summary) = match event.kind {
        e::SESSION_PRESENCE => {
            let presence = &attrs["presence"];
            if presence["state"] == "needs_input" {
                let wait = &presence["wait"];
                let reason = if wait["kind"] == "permission" {
                    "permission_requested"
                } else {
                    "input_requested"
                };
                (
                    reason,
                    true,
                    text(&wait["summary"]).or_else(|| text(&wait["tool"])),
                )
            } else {
                // the wait is over: what nothing else is unread about is done
                conn.execute(
                    "UPDATE notifications SET waiting = 0,
                       done_at = CASE WHEN unread = 0 THEN ?2 ELSE done_at END
                     WHERE session_id = ?1 AND waiting = 1",
                    params![session, iso(event.at)],
                )?;
                if presence["state"] == "stopped" && presence["stop_reason"] == "exited" {
                    ("agent_stopped", false, None)
                } else {
                    return Ok(());
                }
            }
        }
        e::SESSION_TURN => match attrs["end_kind"].as_str() {
            Some("done") => ("agent_finished", false, None),
            Some("failed") => ("agent_failed", false, None),
            _ => return Ok(()),
        },
        e::RUN_ENDED => {
            let failed = matches!(attrs["reason"].as_str(), Some("start_failed" | "host_lost"))
                || attrs["exit_code"].as_i64().is_some_and(|code| code != 0);
            if !failed {
                return Ok(());
            }
            ("agent_stopped", false, None)
        }
        e::SESSION_STATUS if attrs["status"]["kind"] == "note" => {
            // the notice leaves a note's text out: its first line, stored
            let note: Option<String> = conn
                .query_row(
                    "SELECT text FROM session_statuses WHERE id = ?1",
                    params![attrs["status"]["id"].as_i64()],
                    |r| r.get(0),
                )
                .optional()?;
            let first = note.and_then(|t| {
                t.lines()
                    .find(|l| !l.trim().is_empty())
                    .map(|l| l.trim().trim_start_matches('#').trim().to_string())
            });
            ("agent_note", false, first)
        }
        _ => return Ok(()),
    };
    if !waiting && hidden(conn, reason)? {
        return Ok(());
    }
    report(
        conn,
        &Notify {
            subject: &EventSubject::Session(session.to_string()),
            reason,
            waiting,
            event: event.event,
            actor: event.actor,
            summary: summary.as_deref(),
            at: event.at,
        },
    )?;
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
        let filters = NotificationFilters {
            listed,
            ..Default::default()
        };
        Ok(self.notifications_page(&filters, limit, 0)?.0)
    }

    /// A page of what the filters keep, newest first: at most `limit` from
    /// `offset`, and how many they keep in all.
    pub fn notifications_page(
        &self,
        filters: &NotificationFilters,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<(Vec<Notification>, i64)> {
        let (filter, mut args) = filters.sql();
        let conn = self.read();
        let total = conn.query_row(
            &format!("SELECT count(*) FROM notifications n WHERE {filter}"),
            rusqlite::params_from_iter(&args),
            |r| r.get(0),
        )?;
        let n = args.len();
        args.push(limit.to_string());
        args.push(offset.to_string());
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM notifications n LEFT JOIN repos g ON g.id = n.repo_id
              WHERE {filter} ORDER BY n.last_event DESC
              LIMIT CAST(?{} AS INTEGER) OFFSET CAST(?{} AS INTEGER)",
            n + 1,
            n + 2
        ))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(&args), row_to_notification)?
            .collect::<rusqlite::Result<_>>()?;
        Ok((rows, total))
    }

    /// Marks read every notification the filters keep whose latest event is
    /// `up_to` or older: what the person saw listed. Returns how many.
    pub fn read_notifications_up_to(
        &self,
        filters: &NotificationFilters,
        up_to: i64,
    ) -> rusqlite::Result<usize> {
        let (filter, mut args) = filters.sql();
        args.push(up_to.to_string());
        let conn = self.conn();
        conn.execute(
            &format!(
                "UPDATE notifications AS n SET read_event = last_event, unread = 0
                  WHERE {filter} AND n.unread > 0 AND n.last_event <= CAST(?{} AS INTEGER)",
                args.len()
            ),
            rusqlite::params_from_iter(&args),
        )
    }

    /// The events of a notification's subject after `after`, oldest first,
    /// at most `limit`.
    pub fn notification_events(
        &self,
        subject: &EventSubject,
        after: i64,
        limit: usize,
    ) -> rusqlite::Result<Vec<super::Event>> {
        let (column, id) = subject_column(subject);
        self.events_where(&format!("e.{column} = ?1"), id, after, limit)
    }

    /// Takes a notification out of done. `false` when there is none.
    pub fn mark_notification_undone(&self, id: i64) -> rusqlite::Result<bool> {
        let conn = self.conn();
        Ok(conn.execute(
            "UPDATE notifications SET done_at = NULL WHERE id = ?1",
            params![id],
        )? > 0)
    }

    /// Brings the waiting notifications in line with what waits, as of
    /// `now`: a review or a question without an outcome, and an agent whose
    /// presence is `needs_input`. A notification that should wait and does
    /// not is made to wait, one that waits for nothing any more stops, and
    /// a session that finished unseen and has no notification gets an
    /// unread one. Returns how many notifications it changed.
    pub fn reconcile_notifications(&self, now: DateTime<Utc>) -> rusqlite::Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let now = iso(now);
        let mut changed = 0;
        // a review or a question that waits
        changed += tx.execute(
            "INSERT INTO notifications (review_id, repo_id, reason, waiting, last_event,
               last_actor, unread, updated_at)
             SELECT r.id, r.repo_id,
                    CASE WHEN r.kind = 'question' THEN 'question_asked'
                         WHEN r.revises IS NOT NULL THEN 'new_round'
                         ELSE 'review_requested' END,
                    1, COALESCE((SELECT MAX(e.id) FROM events e WHERE e.review_id = r.id), 0),
                    r.requested_by, 1, r.created_at
               FROM reviews r LEFT JOIN outcomes o ON o.review_id = r.id
              WHERE o.review_id IS NULL AND (r.expires_at IS NULL OR r.expires_at > ?1)
             ON CONFLICT (review_id) WHERE review_id IS NOT NULL DO UPDATE SET
               waiting = 1, done_at = NULL
             WHERE waiting = 0 OR done_at IS NOT NULL",
            params![now],
        )?;
        // a review that waits no more: decided, discarded, withdrawn or expired
        changed += tx.execute(
            "UPDATE notifications SET waiting = 0,
               done_at = CASE WHEN EXISTS (SELECT 1 FROM outcomes o WHERE o.review_id = notifications.review_id
                                             AND o.kind IN ('decided', 'discarded'))
                              THEN ?1 ELSE done_at END
             WHERE waiting = 1 AND review_id IS NOT NULL
               AND NOT EXISTS (SELECT 1 FROM reviews r LEFT JOIN outcomes o ON o.review_id = r.id
                                WHERE r.id = notifications.review_id AND o.review_id IS NULL
                                  AND (r.expires_at IS NULL OR r.expires_at > ?1))",
            params![now],
        )?;
        // an agent that waits in its terminal
        changed += tx.execute(
            "INSERT INTO notifications (session_id, repo_id, reason, waiting, last_event,
               last_actor, summary, unread, updated_at)
             SELECT p.session, s.repo_id,
                    CASE WHEN p.wait_kind = 'permission' THEN 'permission_requested'
                         ELSE 'input_requested' END,
                    1, COALESCE((SELECT MAX(e.id) FROM events e WHERE e.session_id = p.session), 0),
                    p.agent, COALESCE(p.wait_summary, p.wait_tool), 1, ?1
               FROM session_presence p JOIN sessions s ON s.agent = p.agent AND s.id = p.session
              WHERE p.state = 'needs_input'
             ON CONFLICT (session_id) WHERE session_id IS NOT NULL DO UPDATE SET
               waiting = 1, reason = excluded.reason, done_at = NULL
             WHERE waiting = 0 OR done_at IS NOT NULL",
            params![now],
        )?;
        // an agent that waits no more
        changed += tx.execute(
            "UPDATE notifications SET waiting = 0
             WHERE waiting = 1 AND session_id IS NOT NULL
               AND NOT EXISTS (SELECT 1 FROM session_presence p
                                WHERE p.session = notifications.session_id
                                  AND p.state = 'needs_input')",
            [],
        )?;
        // a session that finished unseen, as the inbox's Agents section
        // showed it, unless the person turned finished agents off
        if !hidden(&tx, "agent_finished")? {
            changed += tx.execute(
                "INSERT INTO notifications (session_id, repo_id, reason, waiting, last_event,
                   last_actor, unread, updated_at)
                 SELECT p.session, s.repo_id, 'agent_finished', 0,
                        COALESCE((SELECT MAX(e.id) FROM events e WHERE e.session_id = p.session), 0),
                        p.agent, 1, ?1
                   FROM session_presence p JOIN sessions s ON s.agent = p.agent AND s.id = p.session
                  WHERE p.finished_at IS NOT NULL
                    AND (p.seen_at IS NULL OR p.seen_at < p.finished_at)
                    AND NOT EXISTS (SELECT 1 FROM notifications n WHERE n.session_id = p.session)",
                params![now],
            )?;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Keeps the lists short, as of `now`: a read notification that does not
    /// wait is marked done 7 days after its last event, and a done one is
    /// deleted 30 days after it was marked done, unless it is muted, which
    /// it must stay. Returns how many it changed.
    pub fn sweep_notifications(&self, now: DateTime<Utc>) -> rusqlite::Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut changed = tx.execute(
            "UPDATE notifications SET done_at = ?1
             WHERE waiting = 0 AND unread = 0 AND done_at IS NULL AND updated_at < ?2",
            params![iso(now), iso(now - chrono::Duration::days(7))],
        )?;
        changed += tx.execute(
            "DELETE FROM notifications
             WHERE waiting = 0 AND muted = 0 AND done_at IS NOT NULL AND done_at < ?1",
            params![iso(now - chrono::Duration::days(30))],
        )?;
        tx.commit()?;
        Ok(changed)
    }

    /// What is new on a notification's subject since the person last looked:
    /// for a work item, the events after the last one read that the person
    /// did not cause, and the sub-items added to it; for a session, the turns
    /// finished since it was seen.
    pub fn notification_new(&self, n: &Notification) -> rusqlite::Result<New> {
        let conn = self.read();
        let mut new = New::default();
        match &n.subject {
            EventSubject::Item(item) => {
                let mut stmt = conn.prepare(
                    "SELECT id, actor, json_extract(attrs, '$.change'), json_extract(attrs, '$.excerpt')
                       FROM events
                      WHERE item_id = ?1 AND kind = ?2 AND id > ?3
                        AND actor_kind IS NOT 'person'
                      ORDER BY id",
                )?;
                let own: Vec<OwnEvent> = stmt
                    .query_map(
                        params![item, crate::events::ITEM_CHANGED, n.read_event],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                    )?
                    .collect::<rusqlite::Result<_>>()?;
                let mut stmt = conn.prepare(
                    "SELECT e.id, g.prefix || '-' || w.number, w.title
                       FROM events e JOIN work_items w ON w.id = e.item_id
                       JOIN repos g ON g.id = w.repo_id
                      WHERE w.parent = ?1 AND e.kind = ?2 AND e.id > ?3
                        AND json_extract(e.attrs, '$.change') = 'created'
                        AND e.actor_kind IS NOT 'person'
                      ORDER BY e.id",
                )?;
                let subs: Vec<(i64, String, String)> = stmt
                    .query_map(
                        params![item, crate::events::ITEM_CHANGED, n.read_event],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )?
                    .collect::<rusqlite::Result<_>>()?;
                let mut last = (0, None);
                for (id, key, title) in &subs {
                    new.sub_items += 1;
                    new.sub_item = Some((key.clone(), title.clone()));
                    last = last.max((*id, Some("sub_item")));
                }
                let mut created = false;
                for (id, actor, change, excerpt) in &own {
                    match change.as_deref() {
                        Some("created") => created = true,
                        Some("commented") => {
                            new.comments += 1;
                            new.comment = excerpt.clone();
                            if let Some(actor) = actor
                                && !new.authors.contains(actor)
                            {
                                new.authors.push(actor.clone());
                            }
                            last = last.max((*id, Some("comment")));
                        }
                        _ => {
                            new.changes += 1;
                            last = last.max((*id, Some("change")));
                        }
                    }
                }
                new.breakdown = created && new.sub_items > 0;
                new.latest = last.1;
                new.parent = conn
                    .query_row(
                        "SELECT g.prefix || '-' || p.number FROM work_items i
                           JOIN work_items p ON p.id = i.parent JOIN repos g ON g.id = p.repo_id
                          WHERE i.id = ?1",
                        params![item],
                        |r| r.get(0),
                    )
                    .optional()?;
            }
            EventSubject::Session(session) => {
                // the session's own read state: the turns since it was seen
                let (turns, reply): (i64, Option<String>) = conn.query_row(
                    "SELECT count(*),
                            (SELECT x.reply FROM session_turns u
                               JOIN session_turn_replies x ON x.turn_id = u.id
                              WHERE u.session = ?1 AND u.end_kind = 'done'
                              ORDER BY x.at DESC, x.id DESC LIMIT 1)
                       FROM session_turns t
                      WHERE t.session = ?1 AND t.end_kind = 'done'
                        AND t.ended_at > COALESCE((SELECT p.seen_at FROM session_presence p
                                                    WHERE p.session = ?1), '')",
                    params![session],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                new.turns = turns;
                new.reply = reply.as_deref().and_then(first_line);
            }
            EventSubject::Review(_) => {}
        }
        Ok(new)
    }

    /// The agent of the session of this id, by Pinrail's id.
    pub fn session_agent(&self, id: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.read();
        conn.query_row(
            "SELECT agent FROM sessions WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()
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

    /// How many notifications wait for the person, and how many of the
    /// others are unread.
    pub fn notification_counts(&self) -> rusqlite::Result<Counts> {
        let conn = self.read();
        conn.query_row(
            "SELECT (SELECT count(*) FROM notifications WHERE waiting = 1),
                    (SELECT count(*) FROM notifications
                      WHERE unread > 0 AND done_at IS NULL AND waiting = 0)",
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
