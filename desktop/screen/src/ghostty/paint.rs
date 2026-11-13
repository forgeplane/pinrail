//! The first paint: VT bytes that rebuild a terminal's screens in any VT
//! emulator, for a viewer that attaches.
//!
//! Adapted from herdr-core (Apache-2.0), commit ec63ef3,
//! crates/ghostty-vt/src/lib.rs, `Terminal::paint`; changed by Pinrail: ported
//! to the libghostty-vt 1.0 formatter, started with CAN, and ended with the
//! escape sequence the last write left unfinished.

use super::{Terminal, ffi};
use crate::Screen;

/// What the paint starts with. CAN abandons any sequence the viewer was in
/// the middle of. A soft reset leaves the mouse, focus, paste,
/// synchronized output and keyboard modes alone, so they are turned off
/// too: a viewer that missed their "off" then ends up with exactly the
/// screen's own modes, which the formatter replays. Then the primary
/// screen, cleared.
const RESET: &[u8] = b"\x18\x1b[!p\x1b[?2026l\x1b[?9l\x1b[?1000l\x1b[?1001l\x1b[?1002l\x1b[?1003l\
\x1b[?1004l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l\x1b[?2004l\x1b[?1l\x1b[?66l\x1b[?2031l\
\x1b[?2048l\x1b[=0;1u\x1b[?1049l\x1b[?1047l\x1b[?47l\x1b[H\x1b[2J";

/// What the formatter writes that the paint must not pass on: the paint
/// chooses the screen itself, since the alternate screen's aliases can
/// disagree with the active one, and synchronized output belongs to the
/// old viewer and would freeze a new one.
const LEFT_OUT: [&[u8]; 4] = [b"\x1b[?2026h", b"\x1b[?47h", b"\x1b[?1047h", b"\x1b[?1049h"];

impl Terminal {
    /// VT bytes that rebuild this terminal in any VT emulator of the same
    /// size: up to `history` lines of the primary screen's history pushed
    /// into the viewer's own scrollback, the primary screen, and the
    /// alternate screen over it when the program uses one, with styles,
    /// cursor, modes and title, but not the palette, so the viewer's theme
    /// applies. History is best effort, as the library prunes it by page.
    pub(super) fn paint(&self, history: usize) -> Vec<u8> {
        let (cols, rows) = self.size();
        let last = (cols.saturating_sub(1), u32::from(rows).saturating_sub(1));
        let alternate = self.alternate_screen();
        let mut out = RESET.to_vec();
        // The formatter shows only the active screen. For the hidden primary
        // screen, paint a copy switched back to it, which leaves the live
        // terminal and its unfinished input alone.
        if alternate
            && let Some(snapshot) = self.binary_snapshot()
            && let Ok(mut primary) = Terminal::restore(&snapshot.bytes, usize::MAX)
        {
            primary.write(b"\x18\x1b[?1049l");
            out = primary.paint(history);
        }
        let available = self.retained_lines() as u32;
        if history > 0 && available > 0 && !alternate {
            let from = available.saturating_sub(history as u32);
            if let Some(selection) =
                self.select(ffi::POINT_HISTORY, (0, from), (last.0, available - 1))
            {
                out.extend(self.format(ffi::FORMAT_VT, Some(&selection), ffi::EXTRAS_NONE, false));
                // push those lines up into the viewer's scrollback, then
                // clear the screen for the screen's own content
                out.extend(b"\x1b[0m\r\n");
                out.extend(b"\n".repeat(usize::from(rows)));
                out.extend(b"\x1b[H\x1b[2J");
            }
        }
        if alternate {
            // the primary screen's pen, margins and character sets must not
            // carry over into this one
            out.extend(b"\x1b[?1049h\x1b[!p");
        }
        if let Some(selection) = self.select(ffi::POINT_ACTIVE, (0, 0), last) {
            let extras = ffi::EXTRAS_ALL & !ffi::EXTRA_PALETTE;
            let screen = self.format(ffi::FORMAT_VT, Some(&selection), extras, false);
            out.extend(strip(&screen, &LEFT_OUT));
        }
        if let Some(title) = self.title() {
            out.extend(format!("\x1b]2;{title}\x1b\\").as_bytes());
        }
        if let Some(unfinished) = self.continuation() {
            out.extend(unfinished);
        }
        out
    }
}

/// `haystack` without any of `needles`.
fn strip(haystack: &[u8], needles: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(haystack.len());
    let mut i = 0;
    while i < haystack.len() {
        match needles
            .iter()
            .find(|needle| haystack[i..].starts_with(needle))
        {
            Some(needle) => i += needle.len(),
            None => {
                out.push(haystack[i]);
                i += 1;
            }
        }
    }
    out
}
