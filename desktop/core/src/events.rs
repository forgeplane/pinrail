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
/// An agent reported what it is doing in its session, with `pinrail status`.
pub const SESSION_STATUS: &str = "session_status";
/// The person starred a round of a review, or removed its star.
/// The person archived a session, or brought it back.
pub const SESSION_ARCHIVED: &str = "session_archived";
pub const SESSION_UNARCHIVED: &str = "session_unarchived";
pub const STARRED: &str = "starred";
pub const UNSTARRED: &str = "unstarred";
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
