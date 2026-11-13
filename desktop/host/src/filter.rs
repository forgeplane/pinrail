//! The query filter between a program's output and its viewers. The host's
//! screen answers the program's terminal queries, so a viewer must not see
//! a query the screen answered, or the program would get a second answer.
//! The filter decides from what the screen does, not from a list of
//! queries, so it cannot disagree with the library.

/// The longest control sequence held back whole; a longer one passes on.
const CSI_CAP: usize = 64;
/// The longest string sequence (OSC, DCS, APC, PM, SOS) held back whole.
const STRING_CAP: usize = 512;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// ESC alone, waiting for the byte that says what follows.
    Escape,
    /// ESC [, ended by a byte from 0x40 to 0x7E.
    Csi,
    /// ESC ], ESC P, ESC _, ESC ^ or ESC X, ended by BEL (OSC only) or
    /// ESC \.
    String,
    /// Another escape sequence: intermediates, then a final byte.
    Other,
}

/// Holds escape sequences until they are complete, and removes from the
/// viewers' stream each query that the screen answered.
#[derive(Debug, Default)]
pub struct QueryFilter {
    /// The sequence being held, from its ESC.
    held: Vec<u8>,
    kind: Option<Kind>,
}

impl QueryFilter {
    /// Filters a chunk of program output. Everything goes to the screen
    /// through `feed`, which returns how many reply bytes the screen
    /// produced; what the viewers should see is appended to `viewers`.
    pub fn push(
        &mut self,
        chunk: &[u8],
        viewers: &mut Vec<u8>,
        feed: &mut impl FnMut(&[u8]) -> usize,
    ) {
        let mut i = 0;
        while i < chunk.len() {
            let Some(kind) = self.kind else {
                // a plain run up to the next ESC, fed in one call
                let end = chunk[i..]
                    .iter()
                    .position(|&b| b == ESC)
                    .map_or(chunk.len(), |p| i + p);
                if end > i {
                    feed(&chunk[i..end]);
                    viewers.extend_from_slice(&chunk[i..end]);
                }
                if end < chunk.len() {
                    self.held.push(ESC);
                    self.kind = Some(Kind::Escape);
                }
                i = end + 1;
                continue;
            };
            let byte = chunk[i];
            i += 1;
            match kind {
                Kind::Escape => {
                    if byte == ESC {
                        // a lone ESC, then a new sequence
                        self.pass(viewers, feed);
                        self.held.push(ESC);
                        self.kind = Some(Kind::Escape);
                        continue;
                    }
                    if byte == CAN || byte == SUB {
                        self.abandon(byte, viewers, feed);
                        continue;
                    }
                    self.held.push(byte);
                    self.kind = Some(match byte {
                        b'[' => Kind::Csi,
                        b']' | b'P' | b'_' | b'^' | b'X' => Kind::String,
                        0x20..=0x2f => Kind::Other,
                        // ESC and one final byte: complete
                        _ => {
                            self.finish(viewers, feed);
                            continue;
                        }
                    });
                }
                Kind::Csi => {
                    if byte == CAN || byte == SUB || byte == ESC {
                        self.abandon(byte, viewers, feed);
                        continue;
                    }
                    self.held.push(byte);
                    if (0x40..=0x7e).contains(&byte) {
                        self.finish(viewers, feed);
                    } else if self.held.len() > CSI_CAP {
                        self.pass(viewers, feed);
                    }
                }
                Kind::Other => {
                    if byte == CAN || byte == SUB || byte == ESC {
                        self.abandon(byte, viewers, feed);
                        continue;
                    }
                    self.held.push(byte);
                    if !(0x20..=0x2f).contains(&byte) {
                        self.finish(viewers, feed);
                    }
                }
                Kind::String => {
                    if byte == CAN || byte == SUB {
                        self.abandon(byte, viewers, feed);
                        continue;
                    }
                    let after_escape = self.held.last() == Some(&ESC);
                    if after_escape && byte != b'\\' {
                        // an ESC that is not ST ends the string and starts
                        // a new sequence
                        self.held.pop();
                        self.pass(viewers, feed);
                        self.held.push(ESC);
                        self.kind = Some(Kind::Escape);
                        i -= 1;
                        continue;
                    }
                    self.held.push(byte);
                    let osc = self.held.get(1) == Some(&b']');
                    if (after_escape && byte == b'\\') || (osc && byte == BEL) {
                        self.finish(viewers, feed);
                    } else if self.held.len() > STRING_CAP {
                        self.pass(viewers, feed);
                    }
                }
            }
        }
    }

    /// A complete sequence: a query is fed alone and removed from the
    /// viewers' stream if the screen answered it; anything else passes.
    fn finish(&mut self, viewers: &mut Vec<u8>, feed: &mut impl FnMut(&[u8]) -> usize) {
        let held = std::mem::take(&mut self.held);
        self.kind = None;
        let answered = feed(&held) > 0;
        if !(may_be_query(&held) && answered) {
            viewers.extend_from_slice(&held);
        }
    }

    /// A sequence too long to hold: it passes unchanged, and the rest of
    /// it, as it arrives, is plain output.
    fn pass(&mut self, viewers: &mut Vec<u8>, feed: &mut impl FnMut(&[u8]) -> usize) {
        let held = std::mem::take(&mut self.held);
        self.kind = None;
        feed(&held);
        viewers.extend_from_slice(&held);
    }

    /// CAN or SUB cancels the sequence, and an ESC starts a new one: what
    /// was held passes unchanged.
    fn abandon(&mut self, byte: u8, viewers: &mut Vec<u8>, feed: &mut impl FnMut(&[u8]) -> usize) {
        self.pass(viewers, feed);
        if byte == ESC {
            self.held.push(ESC);
            self.kind = Some(Kind::Escape);
        } else {
            feed(&[byte]);
            viewers.push(byte);
        }
    }

    /// The bytes held back, which a first paint ends with.
    pub fn held(&self) -> &[u8] {
        &self.held
    }
}

/// Whether a complete sequence can be a query: a CSI sequence ending in
/// `c`, `n`, `p`, `q`, `t` or `u`, an OSC sequence that contains `?`, or a
/// DCS sequence that starts with `$q` or `+q`.
fn may_be_query(sequence: &[u8]) -> bool {
    match sequence.get(1) {
        Some(b'[') => matches!(
            sequence.last(),
            Some(b'c' | b'n' | b'p' | b'q' | b't' | b'u')
        ),
        Some(b']') => sequence.contains(&b'?'),
        Some(b'P') => sequence[2..].starts_with(b"$q") || sequence[2..].starts_with(b"+q"),
        _ => false,
    }
}
