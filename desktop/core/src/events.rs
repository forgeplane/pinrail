//! Application events shared by reviews, settings and plugins. The UI, tray
//! and review waiters subscribe to the same bus. Every notice is also a row
//! in the events table.

mod service;

pub use service::Events;

use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

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
/// The person archived a session, or brought it back.
pub const SESSION_ARCHIVED: &str = "session_archived";
pub const SESSION_UNARCHIVED: &str = "session_unarchived";
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
/// Reviews past the history's keep-days were deleted.
pub const HISTORY_SWEPT: &str = "history_swept";

#[derive(Debug, Clone, Serialize)]
pub struct Notice {
    pub event_id: i64,
    pub kind: String,
    pub review_id: Option<String>,
    /// The review without its payload, when the notice is about one.
    pub review: Option<Value>,
    /// The settings that changed, as JSON pointers, for `settings_changed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys: Option<Vec<String>>,
    /// The session and its new status, for `session_status`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct Bus {
    tx: broadcast::Sender<Notice>,
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Bus { tx }
    }

    pub fn publish(&self, notice: Notice) {
        let _ = self.tx.send(notice);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Notice> {
        self.tx.subscribe()
    }
}
