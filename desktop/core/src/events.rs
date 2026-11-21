//! Application events shared by reviews, settings and plugins. The UI, tray
//! and review waiters subscribe to the same bus. Every notice is also a row
//! in the events table.

mod service;

pub use service::Events;

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::db::Db;

pub const CREATED: &str = "created";
pub const VIEWED: &str = "viewed";
pub const DECIDED: &str = "decided";
pub const WITHDRAWN: &str = "withdrawn";
pub const EXPIRED: &str = "expired";
pub const DISCARDED: &str = "discarded";
/// A pending review moved to the version of its plugin installed now.
pub const PLUGIN_CHANGED: &str = "plugin_changed";
pub const PLUGINS_RELOADED: &str = "plugins_reloaded";
pub const SETTINGS_CHANGED: &str = "settings_changed";
/// A file in the themes folder was added, changed or removed.
pub const THEMES_CHANGED: &str = "themes_changed";
/// An agent reported what it is doing in its session, with `pinrail status`.
pub const SESSION_STATUS: &str = "session_status";
/// The person sent the agent a message in its session, deleted one still
/// queued, or the agent received the queued ones with a status.
pub const SESSION_MESSAGE_SENT: &str = "session_message_sent";
pub const SESSION_MESSAGE_DELETED: &str = "session_message_deleted";
pub const SESSION_MESSAGES_DELIVERED: &str = "session_messages_delivered";
/// A message to an agent moved to another state, such as typed or held.
pub const SESSION_MESSAGE_STATE: &str = "session_message_state";
/// What an agent's hooks report changed its session's state, wait or
/// finish; a turn opened or ended; the person saw the session, or marked it
/// unread.
pub const SESSION_PRESENCE: &str = "session_presence";
pub const SESSION_TURN: &str = "session_turn";
pub const SESSION_SEEN: &str = "session_seen";
/// The person archived a session, or brought it back.
pub const SESSION_ARCHIVED: &str = "session_archived";
pub const SESSION_UNARCHIVED: &str = "session_unarchived";
/// The person renamed a session, or gave it back the name Pinrail gave it.
pub const SESSION_RENAMED: &str = "session_renamed";
/// The person deleted a session.
pub const SESSION_DELETED: &str = "session_deleted";
/// Pinrail started an agent in a terminal of a host; the run moved to
/// another session, as on `/clear`; the run ended.
pub const RUN_STARTED: &str = "run_started";
pub const RUN_BOUND: &str = "run_bound";
pub const RUN_ENDED: &str = "run_ended";
/// The person starred a round of a review, or removed its star.
pub const STARRED: &str = "starred";
pub const UNSTARRED: &str = "unstarred";
/// The person created, renamed or deleted a project, or moved a
/// repository into or out of one.
pub const PROJECTS_CHANGED: &str = "projects_changed";
/// The person deleted a review, with every round of it.
pub const REVIEW_DELETED: &str = "review_deleted";
/// The person deleted a repository: its reviews, its sessions and its
/// place in a project.
pub const REPOSITORY_DELETED: &str = "repository_deleted";
/// A work item was added or changed, or commented on.
pub const ITEM_CHANGED: &str = "item_changed";
/// The prefix of a repository's work item keys changed, and with it the
/// keys of its items.
pub const PREFIX_CHANGED: &str = "prefix_changed";
/// The person deleted a work item, with its sub-items.
pub const ITEM_DELETED: &str = "item_deleted";
/// Reviews past the history's keep-days were deleted.
pub const HISTORY_SWEPT: &str = "history_swept";

#[derive(Debug, Clone, Serialize)]
pub struct Notice {
    pub event_id: i64,
    pub kind: String,
    /// When the event was recorded.
    pub at: Option<String>,
    /// Who caused it: the person, an agent and its session, or the
    /// system; null when it was not recorded.
    pub actor: Value,
    /// What it is about: a review, a work item or a session; null for
    /// none.
    pub subject: Value,
    pub review_id: Option<String>,
    /// The review without its payload, when the notice is about one.
    pub review: Option<Value>,
    /// The settings that changed, as JSON pointers, for `settings_changed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys: Option<Vec<String>>,
    /// The session, for the `session_*` notices: with its new status, the
    /// message sent or deleted, or the messages delivered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<Value>,
    /// The item, for `item_changed` and `item_deleted`: its key, and the
    /// agent and session that changed it, when an agent did. For
    /// `prefix_changed`, the repository and its new prefix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<Value>,
    /// The run, for the `run_*` notices: the run, and the session it moved
    /// to or the reason it ended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<Value>,
}

impl Notice {
    /// The notice of a stored event, from its kind and the attributes it was
    /// stored with: the same whether it is sent live or replayed later.
    pub fn new(event_id: i64, kind: &str, attrs: &Value) -> Notice {
        let about = |kinds: &[&str]| kinds.contains(&kind).then(|| attrs.clone());
        Notice {
            event_id,
            kind: kind.to_string(),
            at: None,
            actor: Value::Null,
            subject: Value::Null,
            review_id: None,
            review: None,
            keys: attrs.get("keys").and_then(Value::as_array).map(|keys| {
                keys.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            }),
            session: about(&[
                SESSION_STATUS,
                SESSION_MESSAGE_SENT,
                SESSION_MESSAGE_DELETED,
                SESSION_MESSAGES_DELIVERED,
                SESSION_MESSAGE_STATE,
                SESSION_ARCHIVED,
                SESSION_UNARCHIVED,
                SESSION_RENAMED,
                SESSION_DELETED,
                SESSION_PRESENCE,
                SESSION_TURN,
                SESSION_SEEN,
            ]),
            item: about(&[ITEM_CHANGED, ITEM_DELETED, PREFIX_CHANGED]),
            run: about(&[RUN_STARTED, RUN_BOUND, RUN_ENDED]),
        }
    }

    /// The notice of a stored event, with when it was recorded, who
    /// caused it and what it is about.
    pub fn recorded(event: &crate::db::Event) -> Notice {
        let mut notice = Notice::new(event.id, &event.kind, &event.attrs);
        notice.record(event);
        notice
    }

    /// Takes the time, the actor and the subject from the stored event.
    fn record(&mut self, event: &crate::db::Event) {
        self.at = Some(crate::reviews::iso(event.at));
        self.actor = event.actor_json();
        self.subject = event.subject_json();
    }

    /// The notice with the review it is about, without its payload.
    pub fn about(mut self, review_id: Option<String>, review: Option<Value>) -> Notice {
        self.review_id = review_id;
        self.review = review;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Bus {
    tx: broadcast::Sender<Notice>,
    /// The record each notice is read from, for when it was recorded, who
    /// caused it and what it is about, so a live notice says what its
    /// replay will.
    db: Option<Arc<Db>>,
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Bus { tx, db: None }
    }

    /// A bus whose notices take their time, actor and subject from `db`.
    pub(crate) fn recording(db: Arc<Db>) -> Self {
        Bus {
            db: Some(db),
            ..Bus::new()
        }
    }

    pub fn publish(&self, mut notice: Notice) {
        if notice.at.is_none()
            && let Some(db) = &self.db
            && let Ok(Some(event)) = db.event(notice.event_id)
        {
            notice.record(&event);
        }
        let _ = self.tx.send(notice);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Notice> {
        self.tx.subscribe()
    }
}
