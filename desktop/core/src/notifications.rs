//! The person's notifications as an application operation: the lists and
//! the counts the window reads, and the person's changes to the read and
//! done state, each recorded as a `notification_changed` event so that
//! every window agrees.

use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value, json};

use crate::db::{Db, EventActor, EventSubject, Listed, NewEvent, Notification};
use crate::error::Error;
use crate::events::{self, Bus, Notice};
use crate::items::ItemRef;

/// The most notifications a page holds.
pub const PAGE_MAX: usize = 200;

#[derive(Debug)]
pub struct Notifications {
    db: Arc<Db>,
    bus: Bus,
}

/// What the person changes on a notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Read,
    Unread,
    Done,
    Undone,
}

impl Change {
    fn as_str(self) -> &'static str {
        match self {
            Change::Read => "read",
            Change::Unread => "unread",
            Change::Done => "done",
            Change::Undone => "undone",
        }
    }
}

impl Notifications {
    pub(crate) fn new(db: Arc<Db>, bus: Bus) -> Self {
        Notifications { db, bus }
    }

    /// A page of a list, newest first, with how many the list holds, the
    /// counts, and the newest event the list reflects, for *Mark all as
    /// read*.
    pub fn list(&self, listed: Listed, limit: usize, offset: usize) -> Result<Value, Error> {
        // read first, so that an event between the two is left unread
        let up_to = self.db.newest_event_id()?;
        let (rows, total) = self
            .db
            .notifications_page(listed, limit.min(PAGE_MAX), offset)?;
        let notifications = rows
            .iter()
            .map(|n| self.to_json(n))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({
            "notifications": notifications,
            "total": total,
            "counts": self.db.notification_counts()?.to_json(),
            "up_to": up_to,
        }))
    }

    /// How many notifications wait for the person, and how many are unread.
    pub fn counts(&self) -> Result<Value, Error> {
        Ok(self.db.notification_counts()?.to_json())
    }

    /// Changes a notification's read or done state, and tells every window.
    /// A notification that waits for the person is not marked done: what
    /// they decide or answer ends it.
    pub fn change(&self, id: i64, change: Change) -> Result<(), Error> {
        let Some(notification) = self.db.notification(id)? else {
            return Err(Error::NotFound(format!("notification {id}")));
        };
        if change == Change::Done && notification.waiting {
            return Err(Error::Conflict(
                "a notification that waits for you ends when you decide or answer".into(),
            ));
        }
        // a session's read state is the session's own: seeing it, as the
        // sidebar and the Sessions page do, which its notification follows
        if let (EventSubject::Session(session), Change::Read | Change::Unread) =
            (&notification.subject, change)
            && let Some(agent) = self.db.session_agent(session)?
        {
            let now = Utc::now();
            let seen = (change == Change::Read).then_some(now);
            if let Some(Some((event, attrs))) = self.db.set_seen(&agent, session, seen, now)? {
                self.bus
                    .publish(Notice::new(event, events::SESSION_SEEN, &attrs));
                return Ok(());
            }
        }
        match change {
            Change::Read => self.db.mark_notification_read(id)?,
            Change::Unread => self.db.mark_notification_unread(id)?,
            Change::Done => self.db.mark_notification_done(id, Utc::now())?,
            Change::Undone => self.db.mark_notification_undone(id)?,
        };
        self.announce(json!({"id": id, "change": change.as_str()}))
    }

    /// Marks read what a list showed: every notification of it whose
    /// latest event is `up_to` or older. A later event stays unread.
    pub fn read_all(&self, listed: Listed, up_to: i64) -> Result<usize, Error> {
        let read = self.db.read_notifications_up_to(listed, up_to)?;
        self.announce(json!({"change": "read_all", "up_to": up_to}))?;
        Ok(read)
    }

    /// Records the person's change as `notification_changed`, and publishes
    /// it. It names the notification, not its subject, so that the sweep
    /// deletes it with the other notices that only refresh the window.
    fn announce(&self, attrs: Value) -> Result<(), Error> {
        let event_id = self.db.append_event_as(&NewEvent {
            kind: events::NOTIFICATION_CHANGED,
            subject: None,
            actor: EventActor::Person("person".into()),
            attrs: &attrs,
        })?;
        self.bus
            .publish(Notice::new(event_id, events::NOTIFICATION_CHANGED, &attrs));
        Ok(())
    }

    fn to_json(&self, notification: &Notification) -> Result<Value, Error> {
        to_json(&self.db, notification)
    }
}

/// A notification as the window shows it: its subject as it is now, with
/// its repository's project, and who caused the latest event.
pub fn to_json(db: &Db, notification: &Notification) -> Result<Value, Error> {
    {
        let subject = match &notification.subject {
            EventSubject::Item(id) => match db.item(&ItemRef::Id(id.clone()), None)? {
                Ok(item) => json!({"kind": "item", "item": {
                    "id": item.id,
                    "key": item.key.to_string(),
                    "title": item.title,
                    "status": item.status.as_str(),
                }}),
                Err(_) => json!({"kind": "item", "item": {"id": id}}),
            },
            // the reviews and the sessions are reported from a later step
            EventSubject::Review(id) => json!({"kind": "review", "review": {"id": id}}),
            EventSubject::Session(id) => json!({"kind": "session", "session": {"id": id}}),
        };
        let project = match &notification.repo {
            Some(repo) => db.project_of_repo(repo)?.map(|p| p.to_json()),
            None => None,
        };
        let actor = db
            .event(notification.last_event)?
            .map_or(Value::Null, |e| e.actor_json());
        Ok(json!({
            "id": notification.id,
            "subject": subject,
            "repo": notification.repo,
            "project": project,
            "reason": notification.reason,
            "waiting": notification.waiting,
            "actor": actor,
            "summary": notification.summary,
            "unread": notification.unread,
            "read": notification.unread == 0,
            "done": notification.done_at.is_some(),
            "muted": notification.muted,
            "last_event": notification.last_event,
            "updated_at": crate::reviews::iso(notification.updated_at),
        }))
    }
}
