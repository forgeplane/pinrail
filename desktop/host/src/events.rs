//! The host's events: notifications on channel 0 to the controller that
//! subscribed, each with a `seq` that increases within this host instance.

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::frame::Frame;
use crate::outbox::Outbox;
use serde_json::{Value, json};

/// How many recent events a subscriber can resume from.
const RECENT: usize = 1024;

pub struct Events {
    inner: Mutex<Inner>,
}

struct Inner {
    seq: u64,
    recent: VecDeque<(u64, Value)>,
    /// The subscribed connection's id and its outbox.
    subscriber: Option<(u64, Outbox)>,
}

impl Default for Events {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                seq: 0,
                recent: VecDeque::new(),
                subscriber: None,
            }),
        }
    }
}

impl Events {
    /// Sends an event to the subscriber, if any, and keeps it for a
    /// subscriber that resumes.
    pub fn emit(&self, method: &str, mut params: Value) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.seq += 1;
        params["seq"] = json!(inner.seq);
        let message = json!({"jsonrpc": "2.0", "method": method, "params": params});
        if let Some((_, out)) = &inner.subscriber
            && !out.event(&message)
        {
            // too many events wait unread: the subscriber is told, and
            // resubscribes for a new snapshot
            missed(out, inner.seq);
            inner.subscriber = None;
        }
        let seq = inner.seq;
        inner.recent.push_back((seq, message));
        if inner.recent.len() > RECENT {
            inner.recent.pop_front();
        }
    }

    /// Makes `out` the subscriber. Under the same lock as the events, so
    /// that none is lost or sent twice: queues the reply that `reply`
    /// builds from the current `seq`, then the kept events after `after`
    /// when the subscriber resumes this instance.
    pub fn subscribe(
        &self,
        connection: u64,
        out: &Outbox,
        after: Option<u64>,
        reply: impl FnOnce(u64) -> Value,
    ) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        out.reply(&reply(inner.seq));
        if let Some(after) = after {
            for (_, message) in inner.recent.iter().filter(|(seq, _)| *seq > after) {
                if !out.event(message) {
                    missed(out, inner.seq);
                    return;
                }
            }
        }
        inner.subscriber = Some((connection, out.clone()));
    }

    /// Ends the subscription of a connection that closed.
    pub fn unsubscribe(&self, connection: u64) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner
            .subscriber
            .as_ref()
            .is_some_and(|(id, _)| *id == connection)
        {
            inner.subscriber = None;
        }
    }
}

/// Tells a subscriber that it missed events, so it reconciles.
fn missed(out: &Outbox, seq: u64) {
    out.frame(Frame::json(
        &json!({"jsonrpc": "2.0", "method": "missed", "params": {"seq": seq}}),
    ));
}
