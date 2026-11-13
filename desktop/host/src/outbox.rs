//! A connection's outgoing frames: one queue, written by one writer task in
//! order, which counts what waits unread so that channel 0 stays bounded.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;
use tokio::io::AsyncWrite;
use tokio::sync::{Notify, mpsc};

use crate::frame::{self, Frame};

/// Unread events past which the subscriber is told it missed some.
pub const MAX_UNREAD_EVENTS: usize = 8 * 1024 * 1024;
/// Unread replies past which the connection is closed.
pub const MAX_UNREAD_REPLIES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Event,
    Reply,
    Other,
}

struct Queued {
    frame: Frame,
    class: Class,
    size: usize,
}

struct Unread {
    events: AtomicUsize,
    replies: AtomicUsize,
    /// Signalled when the replies pass their bound.
    overflowed: Notify,
}

/// Where a connection's frames are queued. Cloned for every task that
/// sends on the connection.
#[derive(Clone)]
pub struct Outbox {
    queue: mpsc::UnboundedSender<Queued>,
    unread: Arc<Unread>,
}

impl Outbox {
    /// A new outbox and the task that writes it to `writer`, which ends
    /// when every outbox is dropped or a write fails.
    pub fn new<W: AsyncWrite + Unpin + Send + 'static>(
        mut writer: W,
    ) -> (Self, tokio::task::JoinHandle<()>) {
        let (queue, mut receiver) = mpsc::unbounded_channel::<Queued>();
        let unread = Arc::new(Unread {
            events: AtomicUsize::new(0),
            replies: AtomicUsize::new(0),
            overflowed: Notify::new(),
        });
        let counts = unread.clone();
        let writer = tokio::spawn(async move {
            while let Some(queued) = receiver.recv().await {
                if frame::write(&mut writer, &queued.frame).await.is_err() {
                    break;
                }
                match queued.class {
                    Class::Event => counts.events.fetch_sub(queued.size, Ordering::Relaxed),
                    Class::Reply => counts.replies.fetch_sub(queued.size, Ordering::Relaxed),
                    Class::Other => 0,
                };
            }
        });
        (Self { queue, unread }, writer)
    }

    fn send(&self, frame: Frame, class: Class) -> bool {
        let size = frame.payload.len();
        self.queue.send(Queued { frame, class, size }).is_ok()
    }

    /// Queues a frame that no bound applies to.
    pub fn frame(&self, frame: Frame) -> bool {
        self.send(frame, Class::Other)
    }

    /// Queues a reply. Past [`MAX_UNREAD_REPLIES`], the connection is to
    /// close, which [`Outbox::overflowed`] signals.
    pub fn reply(&self, message: &Value) {
        let frame = Frame::json(message);
        let unread = self
            .unread
            .replies
            .fetch_add(frame.payload.len(), Ordering::Relaxed);
        if unread + frame.payload.len() > MAX_UNREAD_REPLIES {
            self.unread.overflowed.notify_one();
            return;
        }
        self.send(frame, Class::Reply);
    }

    /// Queues an event, unless [`MAX_UNREAD_EVENTS`] already wait unread:
    /// then it is dropped, and `false` says so.
    pub fn event(&self, message: &Value) -> bool {
        let frame = Frame::json(message);
        let size = frame.payload.len();
        if self.unread.events.load(Ordering::Relaxed) + size > MAX_UNREAD_EVENTS {
            return false;
        }
        self.unread.events.fetch_add(size, Ordering::Relaxed);
        self.send(frame, Class::Event)
    }

    /// Resolves when the unread replies passed their bound.
    pub async fn overflowed(&self) {
        self.unread.overflowed.notified().await;
    }
}
