//! A terminal's viewers: the terminal channels opened by `terminal.attach`.
//! Each viewer gets a snapshot, then the output as the query filter leaves
//! it, within the credit it returns; one that falls too far behind is sent
//! a fresh snapshot instead. One viewer at a time controls the terminal's
//! size and input.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::frame::{Frame, kind};
use crate::outbox::Outbox;

/// The credit a viewer starts with.
pub const INITIAL_CREDIT: i64 = 256 * 1024;
/// Unsent output past which a viewer is resynchronized.
pub const MAX_PENDING: usize = 4 * 1024 * 1024;
/// The recent output kept for a viewer that resumes.
pub const RING: usize = 1024 * 1024;
/// The history of a resynchronizing snapshot.
pub const RESYNC_HISTORY: usize = 500;
/// The largest DATA frame sent.
const MAX_DATA: usize = 1024 * 1024;

/// Output after this much quiet goes out at once.
pub const QUIET: Duration = Duration::from_millis(5);
/// Otherwise output is gathered until there is this much of it,
pub const BURST_BYTES: usize = 8 * 1024;
/// or for this long, whichever comes first.
pub const BURST_FOR: Duration = Duration::from_millis(2);

/// A viewer's id: its connection and channel.
pub type ViewerId = (u64, u32);

/// What a viewer asks for when it attaches.
#[derive(Debug, Clone)]
pub struct AttachRequest {
    pub cols: u16,
    pub rows: u16,
    /// Lines of history in a `vt` snapshot.
    pub history: usize,
    /// The snapshot formats the viewer reads, best first.
    pub formats: Vec<String>,
    /// The stream offset the viewer has read up to, when it reconnects.
    pub resume_from: Option<u64>,
}

/// Why a viewer needs a snapshot before more output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Needs {
    /// It attached while no snapshot was possible, and gets its first one
    /// with this much history.
    First(usize),
    /// Its unsent output passed [`MAX_PENDING`] and was dropped.
    Resync,
    /// The terminal was resized by another viewer.
    Resized,
}

struct Viewer {
    out: Outbox,
    channel: u32,
    credit: i64,
    /// Frames waiting for credit, in order.
    pending: VecDeque<Frame>,
    pending_bytes: usize,
    /// Until the attach reply has been queued.
    paused: bool,
    needs: Option<Needs>,
    /// The size of the viewer's own terminal.
    size: (u16, u16),
}

/// The output ring: the last [`RING`] bytes of the viewers' stream, and
/// the offset of the byte after them.
#[derive(Default)]
struct Ring {
    bytes: VecDeque<u8>,
    total: u64,
}

impl Ring {
    fn push(&mut self, bytes: &[u8]) {
        self.bytes.extend(bytes);
        self.total += bytes.len() as u64;
        let excess = self.bytes.len().saturating_sub(RING);
        self.bytes.drain(..excess);
    }

    /// The bytes from `offset` on, if the ring still holds them.
    fn since(&self, offset: u64) -> Option<Vec<u8>> {
        let start = self.total - self.bytes.len() as u64;
        if offset < start || offset > self.total {
            return None;
        }
        Some(
            self.bytes
                .iter()
                .skip((offset - start) as usize)
                .copied()
                .collect(),
        )
    }
}

/// Output gathered into fewer, larger frames: a burst of small writes,
/// such as an agent redrawing its footer, costs the viewers one frame
/// rather than one per write. It is not yet in the stream: its offset
/// comes when it goes out.
#[derive(Default)]
struct Gathered {
    bytes: Vec<u8>,
    /// When the first byte of the burst came.
    since: Option<Instant>,
    /// When the last output came.
    last: Option<Instant>,
}

#[derive(Default)]
pub struct Viewers {
    viewers: BTreeMap<ViewerId, Viewer>,
    controller: Option<ViewerId>,
    ring: Ring,
    gathered: Gathered,
}

/// A META frame.
pub fn meta(channel: u32, value: Value) -> Frame {
    Frame {
        channel,
        kind: kind::META,
        payload: value.to_string().into_bytes(),
    }
}

/// DATA frames for `bytes`, none larger than the frame limit allows.
fn data(channel: u32, bytes: &[u8]) -> Vec<Frame> {
    bytes
        .chunks(MAX_DATA)
        .map(|chunk| Frame {
            channel,
            kind: kind::DATA,
            payload: chunk.to_vec(),
        })
        .collect()
}

/// The frames of a snapshot: its start, its bytes, its end, and the bytes
/// a binary snapshot does not hold, which follow it as output.
pub fn snapshot_frames(
    channel: u32,
    format: &str,
    (cols, rows): (u16, u16),
    offset: u64,
    bytes: &[u8],
    after: &[u8],
) -> Vec<Frame> {
    let mut frames = vec![meta(
        channel,
        json!({"type": "snapshot_begin", "format": format, "cols": cols, "rows": rows, "offset": offset}),
    )];
    frames.extend(data(channel, bytes));
    frames.push(meta(channel, json!({"type": "snapshot_end"})));
    frames.extend(data(channel, after));
    frames
}

impl Viewers {
    /// The stream offset of the next output byte, once the output
    /// gathered so far has gone out.
    pub fn offset(&mut self) -> u64 {
        self.send_gathered();
        self.ring.total
    }

    /// The output a viewer that read up to `offset` missed, if the ring
    /// still holds it.
    pub fn since(&mut self, offset: u64) -> Option<Vec<u8>> {
        self.send_gathered();
        self.ring.since(offset)
    }

    /// Adds a viewer, paused until [`Viewers::resume`], with the frames it
    /// gets first.
    pub fn add(&mut self, id: ViewerId, out: Outbox, size: (u16, u16), first: Vec<Frame>) {
        let pending_bytes = first
            .iter()
            .filter(|f| f.kind == kind::DATA)
            .map(|f| f.payload.len())
            .sum();
        self.viewers.insert(
            id,
            Viewer {
                out,
                channel: id.1,
                credit: INITIAL_CREDIT,
                pending: first.into(),
                pending_bytes,
                paused: true,
                needs: None,
                size,
            },
        );
    }

    /// Adds a viewer that attached while no snapshot was possible: it
    /// gets nothing until one is, then a `vt` snapshot with `history`
    /// lines.
    pub fn add_waiting(&mut self, id: ViewerId, out: Outbox, size: (u16, u16), history: usize) {
        self.add(id, out, size, Vec::new());
        if let Some(viewer) = self.viewers.get_mut(&id) {
            viewer.needs = Some(Needs::First(history));
        }
    }

    /// Whether a viewer waits for a snapshot.
    pub fn waiting(&self) -> bool {
        self.viewers.values().any(|v| v.needs.is_some())
    }

    /// The viewers, to flush each.
    pub fn ids(&self) -> Vec<ViewerId> {
        self.viewers.keys().copied().collect()
    }

    /// Starts sending to a viewer whose attach reply has been queued.
    pub fn resume(
        &mut self,
        id: ViewerId,
        paint: &dyn Fn(usize) -> Option<Vec<u8>>,
        size: (u16, u16),
    ) {
        if let Some(viewer) = self.viewers.get_mut(&id) {
            viewer.paused = false;
        }
        self.flush(id, paint, size);
    }

    pub fn remove(&mut self, id: ViewerId) -> bool {
        if self.controller == Some(id) {
            self.controller = None;
        }
        self.viewers.remove(&id).is_some()
    }

    pub fn controller(&self) -> Option<ViewerId> {
        self.controller
    }

    pub fn is_controller(&self, id: ViewerId) -> bool {
        self.controller == Some(id)
    }

    pub fn contains(&self, id: ViewerId) -> bool {
        self.viewers.contains_key(&id)
    }

    pub fn size_of(&self, id: ViewerId) -> Option<(u16, u16)> {
        self.viewers.get(&id).map(|v| v.size)
    }

    pub fn set_size(&mut self, id: ViewerId, size: (u16, u16)) {
        if let Some(viewer) = self.viewers.get_mut(&id) {
            viewer.size = size;
        }
    }

    /// Makes a viewer the controller, and tells it and the one it
    /// replaces. Returns whether it changed.
    pub fn promote(&mut self, id: ViewerId) -> bool {
        if self.controller == Some(id) || !self.viewers.contains_key(&id) {
            return false;
        }
        if let Some(previous) = self.controller.replace(id) {
            self.send_now(previous, json!({"type": "role", "role": "observer"}));
        }
        self.send_now(id, json!({"type": "role", "role": "controller"}));
        true
    }

    /// Makes the controller an observer.
    pub fn release(&mut self, id: ViewerId) {
        if self.controller == Some(id) {
            self.controller = None;
            self.send_now(id, json!({"type": "role", "role": "observer"}));
        }
    }

    /// A role change goes out at once: it is not output, and a viewer
    /// waiting for a snapshot still needs to know it.
    fn send_now(&mut self, id: ViewerId, value: Value) {
        if let Some(viewer) = self.viewers.get_mut(&id) {
            let _ = viewer.out.frame(meta(viewer.channel, value));
        }
    }

    /// Program output, as the filter left it, for every viewer, at `now`.
    /// Output after [`QUIET`] goes out at once; otherwise it is gathered
    /// until [`BURST_BYTES`] or [`BURST_FOR`], whichever comes first.
    pub fn output(&mut self, bytes: &[u8], now: Instant) {
        if bytes.is_empty() {
            return;
        }
        let quiet = self
            .gathered
            .last
            .is_none_or(|last| now.duration_since(last) >= QUIET);
        self.gathered.last = Some(now);
        self.gathered.bytes.extend_from_slice(bytes);
        self.gathered.since.get_or_insert(now);
        if quiet || self.gathered.bytes.len() >= BURST_BYTES {
            self.send_gathered();
        }
    }

    /// When the gathered output is due to go out, if there is any.
    pub fn gathered_due(&self) -> Option<Instant> {
        self.gathered.since.map(|since| since + BURST_FOR)
    }

    /// Sends the output gathered so far to every viewer.
    pub fn send_gathered(&mut self) {
        self.gathered.since = None;
        if self.gathered.bytes.is_empty() {
            return;
        }
        let bytes = std::mem::take(&mut self.gathered.bytes);
        self.ring.push(&bytes);
        for viewer in self.viewers.values_mut() {
            for frame in data(viewer.channel, &bytes) {
                viewer.push(frame);
            }
        }
        // the buffer is kept for the next burst
        self.gathered.bytes = bytes;
        self.gathered.bytes.clear();
    }

    /// A META frame for every viewer, in order with the output.
    pub fn meta_all(&mut self, value: &Value) {
        // anything that is not output sends the output before it
        self.send_gathered();
        for viewer in self.viewers.values_mut() {
            viewer.push(meta(viewer.channel, value.clone()));
        }
    }

    /// The terminal's size changed: the controller is told in order with
    /// the output; every observer gets a fresh snapshot at the new size, as
    /// its pending output was laid out for the old one.
    pub fn resized(&mut self, size: (u16, u16), paint: &dyn Fn(usize) -> Option<Vec<u8>>) {
        self.send_gathered();
        let value = json!({"type": "size", "cols": size.0, "rows": size.1});
        let ids: Vec<ViewerId> = self.viewers.keys().copied().collect();
        for id in ids {
            if self.controller == Some(id) {
                if let Some(viewer) = self.viewers.get_mut(&id) {
                    viewer.push(meta(id.1, value.clone()));
                }
            } else if let Some(viewer) = self.viewers.get_mut(&id) {
                viewer.drop_pending(Needs::Resized);
            }
            self.flush(id, paint, size);
        }
    }

    /// A viewer returned credit.
    pub fn credit(
        &mut self,
        id: ViewerId,
        bytes: u32,
        paint: &dyn Fn(usize) -> Option<Vec<u8>>,
        size: (u16, u16),
    ) {
        if let Some(viewer) = self.viewers.get_mut(&id) {
            viewer.credit += i64::from(bytes);
        }
        self.flush(id, paint, size);
    }

    /// Sends what a viewer's credit allows: first the snapshot it needs,
    /// then its pending frames.
    pub fn flush(
        &mut self,
        id: ViewerId,
        paint: &dyn Fn(usize) -> Option<Vec<u8>>,
        size: (u16, u16),
    ) {
        // a snapshot is cut after the output gathered so far
        if self.viewers.get(&id).is_some_and(|v| v.needs.is_some()) {
            self.send_gathered();
        }
        let offset = self.ring.total;
        let Some(viewer) = self.viewers.get_mut(&id) else {
            return;
        };
        if viewer.paused || viewer.credit <= 0 {
            return;
        }
        if let Some(needs) = viewer.needs {
            let (notice, history) = match needs {
                Needs::First(history) => (None, history),
                Needs::Resync => (Some(json!({"type": "resync"})), RESYNC_HISTORY),
                Needs::Resized => (
                    Some(json!({"type": "size", "cols": size.0, "rows": size.1})),
                    RESYNC_HISTORY,
                ),
            };
            // inside a long escape sequence: the snapshot waits for its end
            let Some(painted) = paint(history) else {
                return;
            };
            viewer.needs = None;
            let mut frames: Vec<Frame> = notice
                .map(|notice| meta(viewer.channel, notice))
                .into_iter()
                .collect();
            frames.extend(snapshot_frames(
                viewer.channel,
                "vt",
                size,
                offset,
                &painted,
                &[],
            ));
            for frame in frames {
                viewer.send(frame);
            }
        }
        while viewer.credit > 0 {
            let Some(frame) = viewer.pending.pop_front() else {
                break;
            };
            if frame.kind == kind::DATA {
                viewer.pending_bytes -= frame.payload.len();
            }
            viewer.send(frame);
        }
    }

    /// Drops the viewers whose connection closed.
    pub fn prune(&mut self) {
        let closed: Vec<ViewerId> = self
            .viewers
            .iter()
            .filter(|(_, v)| v.out.is_closed())
            .map(|(id, _)| *id)
            .collect();
        for id in closed {
            self.remove(id);
        }
    }

    /// The last frames to every viewer, then closes their channels.
    pub fn close_all(&mut self, last: &Value) {
        self.send_gathered();
        for viewer in self.viewers.values_mut() {
            viewer.push(meta(viewer.channel, last.clone()));
            // the end goes out whatever the credit, as nothing follows it
            while let Some(frame) = viewer.pending.pop_front() {
                viewer.send(frame);
            }
            let _ = viewer.out.frame(Frame {
                channel: viewer.channel,
                kind: kind::CLOSE,
                payload: json!({"reason": "exited"}).to_string().into_bytes(),
            });
        }
        self.viewers.clear();
        self.controller = None;
    }
}

impl Viewer {
    /// Sends a frame, or queues it behind the ones waiting.
    fn push(&mut self, frame: Frame) {
        if self.needs.is_some() {
            // superseded by the snapshot the viewer waits for
            return;
        }
        if self.paused || self.credit <= 0 || !self.pending.is_empty() {
            if frame.kind == kind::DATA {
                self.pending_bytes += frame.payload.len();
            }
            self.pending.push_back(frame);
            if self.pending_bytes > MAX_PENDING {
                self.drop_pending(Needs::Resync);
            }
            return;
        }
        self.send(frame);
    }

    fn drop_pending(&mut self, needs: Needs) {
        self.pending.clear();
        self.pending_bytes = 0;
        // a resync already asks for the snapshot a resize would, and a
        // viewer waiting for its first snapshot gets one at the new size
        if self.needs.is_none() || (needs == Needs::Resized && self.needs == Some(Needs::Resync)) {
            self.needs = Some(needs);
        }
    }

    fn send(&mut self, frame: Frame) {
        if frame.kind == kind::DATA {
            self.credit -= frame.payload.len() as i64;
        }
        let _ = self.out.frame(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn output_after_quiet_goes_out_at_once_and_a_burst_is_gathered() {
        let mut viewers = Viewers::default();
        let start = Instant::now();
        viewers.output(b"first", start);
        assert_eq!(viewers.ring.total, 5);
        assert_eq!(viewers.gathered_due(), None);

        // more within 5 ms waits, for 2 ms from the first of it
        viewers.output(b"ab", start + ms(1));
        viewers.output(b"cd", start + ms(2));
        assert_eq!(viewers.ring.total, 5);
        assert_eq!(viewers.gathered_due(), Some(start + ms(3)));
        viewers.send_gathered();
        assert_eq!(viewers.ring.total, 9);
        assert_eq!(viewers.gathered_due(), None);

        // after 5 ms of quiet, at once again
        viewers.output(b"later", start + ms(8));
        assert_eq!(viewers.ring.total, 14);
    }

    #[test]
    fn a_burst_goes_out_once_it_holds_8_kib() {
        let mut viewers = Viewers::default();
        let start = Instant::now();
        viewers.output(b"x", start);
        viewers.output(&[b'y'; BURST_BYTES - 1], start + ms(1));
        assert_eq!(viewers.ring.total, 1);
        viewers.output(b"z", start + ms(1));
        assert_eq!(viewers.ring.total, 1 + BURST_BYTES as u64);
    }

    #[test]
    fn anything_else_sends_the_gathered_output_before_it() {
        let mut viewers = Viewers::default();
        let start = Instant::now();
        viewers.output(b"a", start);
        viewers.output(b"bc", start + ms(1));
        viewers.meta_all(&json!({"type": "title", "text": "t"}));
        assert_eq!(viewers.ring.total, 3);

        viewers.output(b"de", start + ms(2));
        // the offset a snapshot is cut at follows the gathered output
        assert_eq!(viewers.offset(), 5);
    }
}
