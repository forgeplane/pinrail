//! Events as an application operation: who hears them, and what a client
//! that was away missed.

use std::sync::Arc;

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
        let events = self.db.events_after(after, limit)?;
        // each review once, in one query, however many events name it
        let mut ids: Vec<&str> = events
            .iter()
            .filter_map(|e| e.review_id.as_deref())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let reviews = self.db.reviews_by_id(&ids).unwrap_or_default();
        Ok(events
            .into_iter()
            .map(|event| {
                let review = event
                    .review_id
                    .as_deref()
                    .and_then(|id| reviews.get(id))
                    .map(|review| review.to_json(false));
                Notice::new(event.id, &event.kind, &event.attrs).about(event.review_id, review)
            })
            .collect())
    }
}
