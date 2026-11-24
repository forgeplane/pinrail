//! The person's notifications as an application operation: the lists and
//! the counts the window reads, and the person's changes to the read and
//! done state, each recorded as a `notification_changed` event so that
//! every window agrees.

use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value, json};

use crate::db::{Db, EventActor, EventSubject, NewEvent, Notification, NotificationFilters};
use crate::error::Error;
use crate::events::{self, Bus, Notice};
use crate::items::ItemRef;

/// Why a notification is there: the reasons a `kind` filter takes, beside
/// the subject kinds.
pub const REASONS: [&str; 16] = [
    "review_requested",
    "new_round",
    "question_asked",
    "permission_requested",
    "input_requested",
    "review_withdrawn",
    "review_expired",
    "item_added",
    "item_commented",
    "item_closed",
    "item_claimed",
    "item_changed",
    "agent_finished",
    "agent_failed",
    "agent_stopped",
    "agent_note",
];

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
    Mute,
    Unmute,
}

impl Change {
    fn as_str(self) -> &'static str {
        match self {
            Change::Read => "read",
            Change::Unread => "unread",
            Change::Done => "done",
            Change::Undone => "undone",
            Change::Mute => "muted",
            Change::Unmute => "unmuted",
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
    pub fn list(
        &self,
        filters: &NotificationFilters,
        limit: usize,
        offset: usize,
    ) -> Result<Value, Error> {
        // read first, so that an event between the two is left unread
        let up_to = self.db.newest_event_id()?;
        let (rows, total) = self
            .db
            .notifications_page(filters, limit.min(PAGE_MAX), offset)?;
        let notifications = rows
            .iter()
            .map(|n| self.to_json(n))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({
            "notifications": notifications,
            "total": total,
            // each tab's count of what the filters keep, and of everything
            "counts": self.db.notification_counts_of(filters)?.to_json(),
            "all": self.db.notification_counts()?.to_json(),
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
        if matches!(change, Change::Done | Change::Mute) && notification.waiting {
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
            Change::Mute => self.db.mute_notification(id, true, Utc::now())?,
            Change::Unmute => self.db.mute_notification(id, false, Utc::now())?,
        };
        self.announce(json!({"id": id, "change": change.as_str()}))
    }

    /// The notification of a work item, a session or a review, as the list
    /// gives it, for its page to offer Unmute.
    pub fn of(&self, subject: &EventSubject) -> Result<Value, Error> {
        match self.db.notification_of(0, Some(subject))? {
            Some(notification) => self.to_json(&notification),
            None => Err(Error::NotFound("notification".into())),
        }
    }

    /// Mutes or unmutes a work item's or a session's notifications from its
    /// page, whether or not it has a notification yet, and tells the
    /// windows.
    pub fn mute_subject(&self, subject: &EventSubject, muted: bool) -> Result<(), Error> {
        if matches!(subject, EventSubject::Review(_)) {
            return Err(Error::invalid(
                "",
                "a review's notification waits for you: it is not muted",
            ));
        }
        if let Some(id) = self.db.mute_subject(subject, muted, Utc::now())? {
            let change = if muted { Change::Mute } else { Change::Unmute };
            self.announce(json!({"id": id, "change": change.as_str()}))?;
        }
        Ok(())
    }

    /// The events of a notification's subject after `after`, oldest first,
    /// as notices.
    pub fn events(&self, id: i64, after: i64, limit: usize) -> Result<Vec<Notice>, Error> {
        let Some(notification) = self.db.notification(id)? else {
            return Err(Error::NotFound(format!("notification {id}")));
        };
        Ok(self
            .db
            .notification_events(&notification.subject, after, limit.min(PAGE_MAX))?
            .iter()
            .map(Notice::recorded)
            .collect())
    }

    /// Marks done the notifications read for longer than `read_for` and
    /// deletes old done ones, as of `now`, and tells the windows to read
    /// their list again when it changed any.
    pub fn sweep(
        &self,
        now: chrono::DateTime<Utc>,
        read_for: chrono::Duration,
    ) -> Result<usize, Error> {
        let changed = self.db.sweep_notifications(now, read_for)?;
        if changed > 0 {
            self.announce(json!({"change": "swept"}))?;
        }
        Ok(changed)
    }

    /// Repairs the notifications that disagree with what waits, and tells
    /// the windows to read their list again when it changed any. Returns
    /// how many it changed.
    pub fn reconcile(&self) -> Result<usize, Error> {
        let changed = self.db.reconcile_notifications(Utc::now())?;
        if changed > 0 {
            self.announce(json!({"change": "reconciled"}))?;
        }
        Ok(changed)
    }

    /// Marks read what a list showed: every notification of it whose
    /// latest event is `up_to` or older. A later event stays unread.
    pub fn read_all(&self, filters: &NotificationFilters, up_to: i64) -> Result<usize, Error> {
        // a session's read state is the session's own, as in `change`
        let now = Utc::now();
        for (agent, session) in self.db.sessions_to_read_up_to(filters, up_to)? {
            if let Some(Some((event, attrs))) =
                self.db.set_seen(&agent, &session, Some(now), now)?
            {
                self.bus
                    .publish(Notice::new(event, events::SESSION_SEEN, &attrs));
            }
        }
        let read = self.db.read_notifications_up_to(filters, up_to)?;
        self.announce(json!({"change": "read_all", "up_to": up_to}))?;
        Ok(read)
    }

    /// Marks done every read notification that does not wait, and tells the
    /// windows to read their list again when it marked any. Returns how
    /// many it marked.
    pub fn done_read(&self) -> Result<usize, Error> {
        let done = self.db.mark_read_notifications_done(Utc::now())?;
        if done > 0 {
            self.announce(json!({"change": "done_read"}))?;
        }
        Ok(done)
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
            // the review as a listing gives it, without its payload
            EventSubject::Review(id) => {
                let review = db
                    .reviews_by_id(&[id.as_str()])?
                    .remove(id)
                    .map_or_else(|| json!({ "id": id }), |r| r.to_json(false));
                json!({"kind": "review", "review": review})
            }
            // the session as the sessions list gives it
            EventSubject::Session(id) => {
                let session = match db.session_agent(id)? {
                    Some(agent) => db.session(&agent, id, Utc::now())?,
                    None => None,
                };
                let session = session.map_or_else(
                    || json!({ "id": id }),
                    |s| crate::api::reviews::session_json(&s),
                );
                json!({"kind": "session", "session": session})
            }
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
            // what the row's line says changed since the person last looked
            "new": db.notification_new(notification)?.to_json(),
            "read": notification.unread == 0,
            "done": notification.done_at.is_some(),
            "muted": notification.muted,
            "last_event": notification.last_event,
            // the last event the person saw: the new ones come after it
            "read_event": notification.read_event,
            "updated_at": crate::reviews::iso(notification.updated_at),
        }))
    }
}
