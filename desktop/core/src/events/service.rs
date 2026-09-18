//! Events as an application operation: who hears them, and what a client
//! that was away missed.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::broadcast;

use super::{Bus, Notice};
use crate::db::Db;
use crate::error::Error;

/// The event bus and the record behind it. Every service publishes through
/// the bus it is handed; anyone who listens comes through here.
#[derive(Debug)]
pub struct Events {
    bus: Bus,
    db: Arc<Db>,
}

impl Events {
    pub(crate) fn new(db: Arc<Db>) -> Self {
        Events {
            bus: Bus::new(),
            db,
        }
    }

    /// Notices from here on. Subscribe before reading the backlog, so
    /// nothing between the two is missed.
    pub fn subscribe(&self) -> broadcast::Receiver<Notice> {
        self.bus.subscribe()
    }

    pub fn publish(&self, notice: Notice) {
        self.bus.publish(notice);
    }

    /// The bus the other services publish on.
    pub(crate) fn bus(&self) -> Bus {
        self.bus.clone()
    }

    /// Recorded events after `after`, in id order, with at most `limit`
    /// notices. Review data reflects its current state and omits the payload.
    /// Missing or unreadable reviews leave the notice's review data empty.
    pub fn after(&self, after: i64, limit: usize) -> Result<Vec<Notice>, Error> {
        Ok(self
            .db
            .events_after(after, limit)?
            .into_iter()
            .map(|event| Notice {
                event_id: event.id,
                kind: event.kind,
                review: event
                    .review_id
                    .as_deref()
                    .and_then(|id| self.db.get_review(id).ok().flatten())
                    .map(|review| review.to_json(false)),
                review_id: event.review_id,
                keys: event
                    .attrs
                    .get("keys")
                    .and_then(Value::as_array)
                    .map(|keys| {
                        keys.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    }),
            })
            .collect())
    }
}
