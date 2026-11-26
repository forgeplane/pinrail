//! A connection's outgoing frames, written by one writer task, which
//! counts what waits unread so that channel 0 stays bounded. Each channel
//! keeps its frames in order. Channel 0 goes first, and the terminal
//! channels take turns, a DATA frame of at most [`CHUNK`] bytes each, so
//! that a large snapshot for one terminal does not hold back another
//! terminal's output.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;
use tokio::io::AsyncWrite;
use tokio::sync::{Notify, mpsc};

use crate::frame::{self, Frame, kind};

/// Unread events past which the subscriber is told it missed some.
pub const MAX_UNREAD_EVENTS: usize = 8 * 1024 * 1024;
/// Unread replies past which the connection is closed.
pub const MAX_UNREAD_REPLIES: usize = 64 * 1024 * 1024;
/// The most bytes of a terminal channel's DATA written before another
/// channel takes its turn.
pub const CHUNK: usize = 16 * 1024;

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
    /// The bytes of the payload already written, in parts of [`CHUNK`].
    sent: usize,
}

struct Unread {
    events: AtomicUsize,
    replies: AtomicUsize,
    /// Signalled when the replies pass their bound.
    overflowed: Notify,
}

/// The frames queued for the writer, by channel.
#[derive(Default)]
struct Waiting {
    channels: BTreeMap<u32, VecDeque<Queued>>,
    /// The terminal channel that wrote last.
    turn: u32,
}

impl Waiting {
    fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }

    fn push(&mut self, queued: Queued) {
        self.channels
            .entry(queued.frame.channel)
            .or_default()
            .push_back(queued);
    }

    /// The next frame to write: channel 0's, or else the next terminal
    /// channel's after the one that wrote last, cut to [`CHUNK`] bytes of
    /// DATA, encoded for the wire: a part is encoded from where the last
    /// one ended, with no copy of the rest. With it, the class and size of
    /// the queued frame it ends, whose bounds to release.
    fn next(&mut self) -> (Vec<u8>, Option<(Class, usize)>) {
        let channel = if self.channels.contains_key(&frame::CONTROL_CHANNEL) {
            frame::CONTROL_CHANNEL
        } else {
            let after = self.channels.range(self.turn + 1..).next().map(|(c, _)| *c);
            let channel = after
                .or_else(|| self.channels.keys().next().copied())
                .expect("a frame waits");
            self.turn = channel;
            channel
        };
        let queue = self.channels.get_mut(&channel).expect("a frame waits");
        let head = queue.front_mut().expect("a frame waits");
        if channel != frame::CONTROL_CHANNEL
            && head.frame.kind == kind::DATA
            && head.frame.payload.len() - head.sent > CHUNK
        {
            let part = &head.frame.payload[head.sent..head.sent + CHUNK];
            head.sent += CHUNK;
            return (frame::encode(channel, kind::DATA, part), None);
        }
        let queued = queue.pop_front().expect("a frame waits");
        if queue.is_empty() {
            self.channels.remove(&channel);
        }
        let Queued {
            frame,
            class,
            size,
            sent,
        } = queued;
        let bytes = frame::encode(frame.channel, frame.kind, &frame.payload[sent..]);
        (bytes, Some((class, size)))
    }
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
            let mut waiting = Waiting::default();
            loop {
                if waiting.is_empty() {
                    match receiver.recv().await {
                        Some(queued) => waiting.push(queued),
                        None => break,
                    }
                }
                while let Ok(queued) = receiver.try_recv() {
                    waiting.push(queued);
                }
                let (bytes, done) = waiting.next();
                if frame::write_encoded(&mut writer, &bytes).await.is_err() {
                    break;
                }
                let Some((class, size)) = done else { continue };
                match class {
                    Class::Event => counts.events.fetch_sub(size, Ordering::Relaxed),
                    Class::Reply => counts.replies.fetch_sub(size, Ordering::Relaxed),
                    Class::Other => 0,
                };
            }
        });
        (Self { queue, unread }, writer)
    }

    fn send(&self, frame: Frame, class: Class) -> bool {
        let size = frame.payload.len();
        self.queue
            .send(Queued {
                frame,
                class,
                size,
                sent: 0,
            })
            .is_ok()
    }

    /// Whether the connection's writer has gone.
    pub fn is_closed(&self) -> bool {
        self.queue.is_closed()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn data(channel: u32, payload: Vec<u8>) -> Queued {
        let size = payload.len();
        Queued {
            frame: Frame {
                channel,
                kind: kind::DATA,
                payload,
            },
            class: Class::Other,
            size,
            sent: 0,
        }
    }

    /// The channel and payload of an encoded frame.
    fn decoded(bytes: &[u8]) -> (u32, Vec<u8>) {
        let channel = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let length = u32::from_le_bytes(bytes[5..9].try_into().unwrap()) as usize;
        assert_eq!(bytes.len(), 9 + length);
        (channel, bytes[9..].to_vec())
    }

    #[test]
    fn a_large_frame_goes_out_in_parts_that_take_turns_with_other_channels() {
        let big: Vec<u8> = (0..40 * 1024).map(|i| (i % 251) as u8).collect();
        let mut waiting = Waiting::default();
        waiting.push(data(1, big.clone()));
        waiting.push(data(2, b"other".to_vec()));
        let mut sent = Vec::new();
        while !waiting.is_empty() {
            let (bytes, done) = waiting.next();
            let (channel, payload) = decoded(&bytes);
            sent.push((channel, payload.len(), done.is_some()));
        }
        assert_eq!(
            sent,
            [
                (1, CHUNK, false),
                (2, 5, true),
                (1, CHUNK, false),
                (1, 8 * 1024, true),
            ]
        );
        // the parts join into the frame as queued
        let mut waiting = Waiting::default();
        waiting.push(data(1, big.clone()));
        let mut joined = Vec::new();
        while !waiting.is_empty() {
            joined.extend(decoded(&waiting.next().0).1);
        }
        assert_eq!(joined, big);
    }
}
