//! The append-only record of what happened: every notice the app broadcasts
//! is a row here first, so a client that was away can catch up. An event
//! names what it is about, a review, a work item or a session, and who
//! caused it.

use std::cell::RefCell;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use super::Db;
use crate::reviews::parse_datetime;

/// What an event is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventSubject {
    /// A review, by its id.
    Review(String),
    /// A work item, by its id.
    Item(String),
    /// A session, by Pinrail's id.
    Session(String),
}

/// Who caused an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventActor {
    /// Not recorded, as on the events written before actors were.
    Unknown,
    /// The person, by name.
    Person(String),
    /// An agent, and the session it acted in, by Pinrail's id.
    Agent {
        name: String,
        session: Option<String>,
    },
    /// Pinrail itself, as when a review expires.
    System,
}

/// An event to record.
#[derive(Debug, Clone)]
pub struct NewEvent<'a> {
    pub kind: &'a str,
    pub subject: Option<EventSubject>,
    pub actor: EventActor,
    pub attrs: &'a Value,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub id: i64,
    pub review_id: Option<String>,
    pub kind: String,
    /// The actor's name: the person's or the agent's.
    pub actor: Option<String>,
    pub at: DateTime<Utc>,
    pub attrs: Value,
    pub item_id: Option<String>,
    pub session_id: Option<String>,
    /// `person`, `agent` or `system`; none on older events.
    pub actor_kind: Option<String>,
    pub actor_session: Option<String>,
    /// The subject item's key, read with the event.
    pub item_key: Option<String>,
    /// The subject session's agent, read with the event.
    pub session_agent: Option<String>,
}

impl Event {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "review_id": self.review_id,
            "kind": self.kind,
            "actor": self.actor,
            "at": crate::reviews::iso(self.at),
            "attrs": self.attrs,
        })
    }

    /// What the event is about: its review, item or session.
    pub fn subject(&self) -> Option<EventSubject> {
        if let Some(id) = &self.review_id {
            Some(EventSubject::Review(id.clone()))
        } else if let Some(id) = &self.item_id {
            Some(EventSubject::Item(id.clone()))
        } else {
            self.session_id.clone().map(EventSubject::Session)
        }
    }

    /// Who caused the event, as notices give it: null when it was not
    /// recorded.
    pub fn actor_json(&self) -> Value {
        match self.actor_kind.as_deref() {
            Some("person") => json!({"kind": "person", "name": self.actor}),
            Some("agent") => {
                json!({"kind": "agent", "name": self.actor, "session": self.actor_session})
            }
            Some("system") => json!({"kind": "system"}),
            _ => Value::Null,
        }
    }

    /// What the event is about, as notices give it: a review, a work item
    /// with its key, or a session with its agent; null for none.
    pub fn subject_json(&self) -> Value {
        if let Some(id) = &self.review_id {
            json!({"kind": "review", "id": id})
        } else if let Some(id) = &self.item_id {
            json!({"kind": "item", "id": id, "key": self.item_key})
        } else if let Some(id) = &self.session_id {
            json!({"kind": "session", "id": id, "agent": self.session_agent})
        } else {
            Value::Null
        }
    }
}

thread_local! {
    /// The ids of the events written on this thread, for each `recording`
    /// that is under way, innermost last.
    static RECORDING: RefCell<Vec<Vec<i64>>> = const { RefCell::new(Vec::new()) };
}

/// Runs `write`, and answers the ids of the events it wrote, in order, so
/// that the caller can publish them once the write committed. An id whose
/// transaction rolled back names no event.
pub fn recording<T>(write: impl FnOnce() -> T) -> (T, Vec<i64>) {
    struct Pop;
    impl Drop for Pop {
        fn drop(&mut self) {
            RECORDING.with(|r| r.borrow_mut().pop());
        }
    }
    RECORDING.with(|r| r.borrow_mut().push(Vec::new()));
    let pop = Pop;
    let answer = write();
    let ids = RECORDING.with(|r| r.borrow_mut().last_mut().map(std::mem::take));
    drop(pop);
    (answer, ids.unwrap_or_default())
}

impl EventActor {
    /// An agent, in its session by Pinrail's id.
    pub fn agent(name: &str, session: &str) -> Self {
        EventActor::Agent {
            name: name.to_string(),
            session: Some(session.to_string()),
        }
    }

    /// The person, by name, or not recorded when the name is not known.
    pub fn person(name: Option<&str>) -> Self {
        name.map_or(EventActor::Unknown, |name| {
            EventActor::Person(name.to_string())
        })
    }
}

impl From<&crate::items::Actor> for EventActor {
    fn from(actor: &crate::items::Actor) -> Self {
        match actor {
            crate::items::Actor::Person => EventActor::Person(actor.name().to_string()),
            crate::items::Actor::Agent { name, session } => EventActor::Agent {
                name: name.clone(),
                session: session.clone(),
            },
        }
    }
}

/// The columns an event is read with, and the joins that give its subject's
/// key and agent.
const SELECT: &str = "SELECT e.id, e.review_id, e.kind, e.actor, e.at, e.attrs, e.item_id,
       e.session_id, e.actor_kind, e.actor_session, g.prefix || '-' || w.number, s.agent
     FROM events e
     LEFT JOIN work_items w ON w.id = e.item_id
     LEFT JOIN repos g ON g.id = w.repo_id
     LEFT JOIN sessions s ON s.id = e.session_id";

impl Db {
    pub fn append_event(
        &self,
        review_id: Option<&str>,
        kind: &str,
        actor: Option<&str>,
        attrs: &Value,
    ) -> rusqlite::Result<i64> {
        let conn = self.conn();
        insert_event(&conn, review_id, kind, actor, attrs)
    }

    /// Records an event with its subject and actor.
    pub fn append_event_as(&self, event: &NewEvent<'_>) -> rusqlite::Result<i64> {
        let conn = self.conn();
        insert_event_as(&conn, event)
    }

    pub fn events_for(&self, review_id: &str) -> rusqlite::Result<Vec<Event>> {
        let conn = self.read();
        let mut stmt = conn.prepare(&format!("{SELECT} WHERE e.review_id = ?1 ORDER BY e.id"))?;
        let rows = stmt.query_map(params![review_id], row_to_event)?;
        rows.collect()
    }

    pub fn events_after(&self, after: i64, limit: usize) -> rusqlite::Result<Vec<Event>> {
        let conn = self.read();
        let mut stmt = conn.prepare(&format!("{SELECT} WHERE e.id > ?1 ORDER BY e.id LIMIT ?2"))?;
        let rows = stmt.query_map(params![after, limit as i64], row_to_event)?;
        rows.collect()
    }

    /// The events after `after` that `condition` keeps, with `value` as its
    /// `?1`, oldest first, at most `limit`.
    pub(super) fn events_where(
        &self,
        condition: &str,
        value: &str,
        after: i64,
        limit: usize,
    ) -> rusqlite::Result<Vec<Event>> {
        let conn = self.read();
        let mut stmt = conn.prepare(&format!(
            "{SELECT} WHERE {condition} AND e.id > ?2 ORDER BY e.id LIMIT ?3"
        ))?;
        let rows = stmt.query_map(params![value, after, limit as i64], row_to_event)?;
        rows.collect()
    }

    /// Deletes the events that are no longer worth replaying, as of `now`,
    /// and returns how many went. An event about a review, an item or a
    /// session stays as long as its subject, except the session's presence
    /// and the person seeing it, whose current state the session keeps. A
    /// notice that only tells the app to read something again goes after
    /// 14 days, and the notice of a deletion after 90.
    pub fn delete_old_events(&self, now: DateTime<Utc>) -> rusqlite::Result<usize> {
        use crate::events as e;
        let list = |kinds: &[&str]| {
            kinds
                .iter()
                .map(|k| format!("'{k}'"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let refresh = list(&[
            e::SETTINGS_CHANGED,
            e::THEMES_CHANGED,
            e::PLUGINS_RELOADED,
            e::PROJECTS_CHANGED,
            e::PREFIX_CHANGED,
            e::HISTORY_SWEPT,
            e::NOTIFICATION_CHANGED,
        ]);
        let presence = list(&[e::SESSION_PRESENCE, e::SESSION_SEEN]);
        let deletions = list(&[
            e::ITEM_DELETED,
            e::SESSION_DELETED,
            e::REVIEW_DELETED,
            e::REPOSITORY_DELETED,
        ]);
        let conn = self.conn();
        conn.execute(
            &format!(
                "DELETE FROM events
                  WHERE (kind IN ({refresh}) AND at < ?1 AND review_id IS NULL
                         AND item_id IS NULL AND session_id IS NULL)
                     OR (kind IN ({presence}) AND at < ?1)
                     OR (kind IN ({deletions}) AND at < ?2)"
            ),
            params![
                crate::reviews::iso(now - chrono::Duration::days(14)),
                crate::reviews::iso(now - chrono::Duration::days(90)),
            ],
        )
    }

    /// Whether the stored events still reach back to `after`: every event
    /// after it is stored, unless something deleted it with its subject.
    /// Otherwise, the id of the newest event ever recorded.
    pub fn events_missed_since(&self, after: i64) -> rusqlite::Result<Option<i64>> {
        let conn = self.read();
        let (oldest, newest): (Option<i64>, i64) = conn.query_row(
            "SELECT (SELECT MIN(id) FROM events),
                    COALESCE((SELECT seq FROM sqlite_sequence WHERE name = 'events'), 0)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let reaches = match oldest {
            Some(oldest) => oldest <= after + 1,
            None => newest <= after,
        };
        Ok((!reaches).then_some(newest))
    }

    /// The event of this id, if it is still recorded.
    pub fn event(&self, id: i64) -> rusqlite::Result<Option<Event>> {
        let conn = self.read();
        conn.query_row(
            &format!("{SELECT} WHERE e.id = ?1"),
            params![id],
            row_to_event,
        )
        .optional()
    }
}

/// Records an event about a review or about nothing, with the actor's name
/// alone, as events were recorded before they had a kind of actor.
pub(super) fn insert_event(
    conn: &Connection,
    review_id: Option<&str>,
    kind: &str,
    actor: Option<&str>,
    attrs: &Value,
) -> rusqlite::Result<i64> {
    let subject = review_id.map(|id| EventSubject::Review(id.to_owned()));
    insert(conn, kind, attrs, subject.as_ref(), (None, actor, None))
}

/// Records an event with its subject and actor.
pub(super) fn insert_event_as(conn: &Connection, event: &NewEvent<'_>) -> rusqlite::Result<i64> {
    let actor = match &event.actor {
        EventActor::Unknown => (None, None, None),
        EventActor::Person(name) => (Some("person"), Some(name.as_str()), None),
        EventActor::Agent { name, session } => {
            (Some("agent"), Some(name.as_str()), session.as_deref())
        }
        EventActor::System => (Some("system"), None, None),
    };
    insert(conn, event.kind, event.attrs, event.subject.as_ref(), actor)
}

/// Records an event about a session, by Pinrail's id.
pub(super) fn session_event(
    conn: &Connection,
    session: &str,
    kind: &str,
    actor: EventActor,
    attrs: &Value,
) -> rusqlite::Result<i64> {
    let subject = Some(EventSubject::Session(session.to_string()));
    insert_event_as(
        conn,
        &NewEvent {
            kind,
            subject,
            actor,
            attrs,
        },
    )
}

/// Records an event about a review.
pub(super) fn review_event(
    conn: &Connection,
    review: &str,
    kind: &str,
    actor: EventActor,
    attrs: &Value,
) -> rusqlite::Result<i64> {
    let subject = Some(EventSubject::Review(review.to_string()));
    insert_event_as(
        conn,
        &NewEvent {
            kind,
            subject,
            actor,
            attrs,
        },
    )
}

/// Writes the row: the actor as its kind, its name and its session.
fn insert(
    conn: &Connection,
    kind: &str,
    attrs: &Value,
    subject: Option<&EventSubject>,
    (actor_kind, actor, actor_session): (Option<&str>, Option<&str>, Option<&str>),
) -> rusqlite::Result<i64> {
    let (review_id, item_id, session_id) = match subject {
        Some(EventSubject::Review(id)) => (Some(id.as_str()), None, None),
        Some(EventSubject::Item(id)) => (None, Some(id.as_str()), None),
        Some(EventSubject::Session(id)) => (None, None, Some(id.as_str())),
        None => (None, None, None),
    };
    // anything that happens to a review, other than the person opening it
    // or starring it, is activity in its session
    if let Some(review_id) = review_id
        && ![
            crate::events::VIEWED,
            crate::events::STARRED,
            crate::events::UNSTARRED,
        ]
        .contains(&kind)
    {
        super::sessions::touch_review(conn, review_id)?;
    }
    let at = Utc::now();
    conn.execute(
        "INSERT INTO events (review_id, kind, actor, at, attrs, item_id, session_id, actor_kind,
           actor_session)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            review_id,
            kind,
            actor,
            crate::reviews::iso(at),
            if attrs.is_null() {
                None
            } else {
                Some(attrs.to_string())
            },
            item_id,
            session_id,
            actor_kind,
            actor_session,
        ],
    )?;
    let id = conn.last_insert_rowid();
    // what the event means for the person's notifications, in its transaction
    super::notifications::project(
        conn,
        &super::notifications::Projected {
            event: id,
            kind,
            subject,
            by_person: actor_kind == Some("person"),
            actor,
            attrs,
            at,
        },
    )?;
    RECORDING.with(|r| {
        if let Some(ids) = r.borrow_mut().last_mut() {
            ids.push(id);
        }
    });
    Ok(id)
}

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    let at: String = row.get(4)?;
    let attrs: Option<String> = row.get(5)?;
    Ok(Event {
        id: row.get(0)?,
        review_id: row.get(1)?,
        kind: row.get(2)?,
        actor: row.get(3)?,
        at: parse_datetime(&at).unwrap_or_default(),
        attrs: attrs
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null),
        item_id: row.get(6)?,
        session_id: row.get(7)?,
        actor_kind: row.get(8)?,
        actor_session: row.get(9)?,
        item_key: row.get(10)?,
        session_agent: row.get(11)?,
    })
}
