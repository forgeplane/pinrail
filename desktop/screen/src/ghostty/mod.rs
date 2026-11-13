//! libghostty-vt, through a safe interface: one [`Terminal`] per PTY,
//! created with the options the host needs.

// The library is reached through its C API, which Rust can only call with
// `unsafe`. This module and `ffi` are the only code in the workspace that
// allows it; each call says why it is sound.
#![allow(unsafe_code)]

mod ffi;

use std::ffi::{CStr, c_void};
use std::fmt;
use std::ptr;

/// The most lines of history a terminal keeps.
pub const MAX_SCROLLBACK_LINES: usize = 100_000;
/// The bytes of history allowed for every 10,000 lines: about six times
/// what 10,000 lines of 120 columns take, so that the line limit applies in
/// normal use and the byte limit only stops a very wide or heavily styled
/// terminal from growing without bound.
pub const SCROLLBACK_BYTES_PER_10K_LINES: usize = 64 * 1024 * 1024;
/// The longest unfinished escape sequence kept, so that a snapshot can be
/// taken while the parser is inside one.
pub const CONTINUATION_MAX_BYTES: usize = 64 * 1024;
/// Grapheme clustering (DEC private mode 2027), so that emoji with VS16 and
/// ZWJ sequences take the width the agents' renderers expect.
const GRAPHEME_CLUSTER: ffi::Mode = ffi::mode(2027, false);

/// The version of the linked libghostty-vt, such as "1.0.0".
pub fn version() -> &'static str {
    // SAFETY: the library returns a pointer to a static, NUL-terminated
    // string that lives as long as the program.
    let version = unsafe { CStr::from_ptr(ffi::ghostty_version()) };
    version.to_str().unwrap_or("")
}

/// A call to the library that failed, with its result code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error {
    pub call: &'static str,
    pub code: i32,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self.code {
            ffi::OUT_OF_MEMORY => "out of memory",
            ffi::INVALID_VALUE => "invalid value",
            _ => "failed",
        };
        write!(f, "libghostty-vt {}: {what} ({})", self.call, self.code)
    }
}

impl std::error::Error for Error {}

fn check(call: &'static str, code: ffi::GhosttyResult) -> Result<(), Error> {
    match code {
        ffi::SUCCESS => Ok(()),
        code => Err(Error { call, code }),
    }
}

/// How a terminal is created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub cols: u16,
    pub rows: u16,
    /// Lines of history to keep, at most [`MAX_SCROLLBACK_LINES`]. The
    /// library prunes by page, so it may keep somewhat fewer or more.
    pub scrollback_lines: usize,
}

impl Options {
    /// The byte limit of the history: [`SCROLLBACK_BYTES_PER_10K_LINES`]
    /// for every 10,000 lines of the line limit.
    pub fn scrollback_bytes(&self) -> usize {
        self.lines() * SCROLLBACK_BYTES_PER_10K_LINES / 10_000
    }

    fn lines(&self) -> usize {
        self.scrollback_lines.min(MAX_SCROLLBACK_LINES)
    }
}

/// One terminal's screen and history.
pub struct Terminal {
    raw: ffi::Terminal,
}

// SAFETY: the library has no thread affinity. A terminal must not be used
// from two threads at once, which `&mut self` on every change and the
// absence of `Sync` ensure.
unsafe impl Send for Terminal {}

impl Terminal {
    pub fn new(options: &Options) -> Result<Self, Error> {
        if options.cols == 0 || options.rows == 0 {
            return Err(Error {
                call: "ghostty_terminal_new",
                code: ffi::INVALID_VALUE,
            });
        }
        let mut raw = ptr::null_mut();
        // SAFETY: a null allocator selects the library's default, and
        // `raw` is a valid place for the handle.
        check("ghostty_terminal_new", unsafe {
            ffi::ghostty_terminal_new(ptr::null(), &mut raw, options.cols, options.rows)
        })?;
        // from here on, Drop frees the handle if a setting fails
        let terminal = Terminal { raw };
        terminal.set_size(ffi::OPT_SCROLLBACK_MAX_LINES, options.lines())?;
        terminal.set_size(ffi::OPT_SCROLLBACK_MAX_BYTES, options.scrollback_bytes())?;
        terminal.set_size(ffi::OPT_CONTINUATION_MAX_BYTES, CONTINUATION_MAX_BYTES)?;
        // the reset default, so that a reset by the program keeps it
        let grapheme = ffi::ModeConfig {
            mode: GRAPHEME_CLUSTER,
            value: true,
        };
        // SAFETY: the option takes a pointer to a GhosttyTerminalModeConfig,
        // read during the call.
        check("ghostty_terminal_set(MODE_DEFAULT)", unsafe {
            ffi::ghostty_terminal_set(
                terminal.raw,
                ffi::OPT_MODE_DEFAULT,
                (&raw const grapheme).cast(),
            )
        })?;
        Ok(terminal)
    }

    /// Sets an option whose input type is `size_t*`.
    fn set_size(&self, option: ffi::TerminalOption, value: usize) -> Result<(), Error> {
        // SAFETY: each option passed here takes a pointer to a size_t, read
        // during the call.
        check("ghostty_terminal_set", unsafe {
            ffi::ghostty_terminal_set(self.raw, option, (&raw const value).cast())
        })
    }

    /// Reads a data item into a value of its output type.
    fn get<T: Default>(&self, data: ffi::TerminalData) -> T {
        let mut value = T::default();
        // SAFETY: each data item read through here has `T` as its output
        // type, so the library writes a valid `T`. A failed read leaves the
        // default.
        unsafe { ffi::ghostty_terminal_get(self.raw, data, (&raw mut value).cast::<c_void>()) };
        value
    }

    /// Parses program output into the screen.
    pub fn write(&mut self, bytes: &[u8]) {
        // SAFETY: the pointer and length describe `bytes`, which the library
        // reads during the call only.
        unsafe { ffi::ghostty_terminal_vt_write(self.raw, bytes.as_ptr(), bytes.len()) }
    }

    /// Changes the size in cells, and the cell size in pixels that image
    /// protocols and size reports use, which stays as it was when `None`.
    pub fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_px: Option<(u16, u16)>,
    ) -> Result<(), Error> {
        if cols == 0 || rows == 0 {
            return Err(Error {
                call: "ghostty_terminal_resize",
                code: ffi::INVALID_VALUE,
            });
        }
        let (width, height) = match cell_px {
            Some((w, h)) => (u32::from(w), u32::from(h)),
            None => self.cell_px(),
        };
        // SAFETY: the handle is valid, and the sizes are nonzero.
        check("ghostty_terminal_resize", unsafe {
            ffi::ghostty_terminal_resize(self.raw, cols, rows, width, height)
        })
    }

    /// The cell size in pixels, from the terminal's size in pixels.
    fn cell_px(&self) -> (u32, u32) {
        let (cols, rows) = self.size();
        let width: u32 = self.get(ffi::DATA_WIDTH_PX);
        let height: u32 = self.get(ffi::DATA_HEIGHT_PX);
        (
            width / u32::from(cols.max(1)),
            height / u32::from(rows.max(1)),
        )
    }

    /// The size in cells, as columns and rows.
    pub fn size(&self) -> (u16, u16) {
        (self.get(ffi::DATA_COLS), self.get(ffi::DATA_ROWS))
    }

    /// The cursor's column and row on the screen, from zero.
    pub fn cursor(&self) -> (u16, u16) {
        (self.get(ffi::DATA_CURSOR_X), self.get(ffi::DATA_CURSOR_Y))
    }

    /// The lines of history the terminal holds now.
    pub fn retained_lines(&self) -> usize {
        self.get(ffi::DATA_SCROLLBACK_ROWS)
    }

    /// Whether a mode is set, by its number, for an ANSI mode or a DEC
    /// private one.
    pub fn mode(&self, number: u16, ansi: bool) -> bool {
        let mut config = ffi::ModeConfig {
            mode: ffi::mode(number, ansi),
            value: false,
        };
        // SAFETY: DATA_MODE reads and writes a GhosttyTerminalModeConfig
        // whose `mode` is set first. An unknown mode fails and leaves false.
        unsafe { ffi::ghostty_terminal_get(self.raw, ffi::DATA_MODE, (&raw mut config).cast()) };
        config.value
    }

    /// The bytes of an escape sequence or UTF-8 character that the last
    /// write left unfinished, or `None` when there is none to replay.
    pub fn continuation(&self) -> Option<Vec<u8>> {
        let (mut out, mut len) = (ptr::null_mut(), 0usize);
        // SAFETY: a null allocator selects the default; on success the
        // library hands over `len` bytes at `out`, which are copied and then
        // freed with the same allocator.
        unsafe {
            if ffi::ghostty_terminal_continuation_alloc(self.raw, ptr::null(), &mut out, &mut len)
                != ffi::SUCCESS
            {
                return None;
            }
            if out.is_null() {
                return Some(Vec::new());
            }
            let bytes = std::slice::from_raw_parts(out, len).to_vec();
            ffi::ghostty_free(ptr::null(), out, len);
            Some(bytes)
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // SAFETY: the handle came from ghostty_terminal_new and is freed once.
        unsafe { ffi::ghostty_terminal_free(self.raw) }
    }
}
