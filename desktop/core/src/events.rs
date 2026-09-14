//! What the server tells listeners as reviews change: the UI, the tray, and
//! waiters inside the API. Every notice is also a row in the events table.

use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

pub const CREATED: &str = "created";
pub const VIEWED: &str = "viewed";
pub const DECIDED: &str = "decided";
pub const WITHDRAWN: &str = "withdrawn";
pub const EXPIRED: &str = "expired";
pub const DISCARDED: &str = "discarded";
pub const PLUGINS_RELOADED: &str = "plugins_reloaded";
pub const SETTINGS_CHANGED: &str = "settings_changed";

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
