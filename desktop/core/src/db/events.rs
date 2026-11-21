//! The append-only record of what happened: every notice the app broadcasts
//! is a row here first, so a client that was away can catch up. An event
//! names what it is about, a review, a work item or a session, and who
//! caused it.

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
    conn.execute(
        "INSERT INTO events (review_id, kind, actor, at, attrs, item_id, session_id, actor_kind,
           actor_session)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            review_id,
            kind,
            actor,
            crate::reviews::iso(Utc::now()),
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
    Ok(conn.last_insert_rowid())
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
