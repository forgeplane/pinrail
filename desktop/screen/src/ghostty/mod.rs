//! libghostty-vt, through a safe interface: one [`Terminal`] per PTY,
//! created with the options the host needs.

// The library is reached through its C API, which Rust can only call with
// `unsafe`. This module and `ffi` are the only code in the workspace that
// allows it; each call says why it is sound.
#![allow(unsafe_code)]

mod ffi;
mod paint;

use std::ffi::{CStr, c_void};
use std::fmt;
use std::ptr;

use crate::{
    BinarySnapshot, CONTINUATION_MAX_BYTES, Cell, Color, ColorScheme, Cursor, DisplayFacts,
    InputModes, MouseFormat, MouseTracking, Options, ProgramState, ProgramStatus, ProgramWait, Rgb,
    Screen, Signal, Style, TextRange, Underline,
};

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

/// Focus reporting (DEC private mode 1004): the program wants `CSI I` and
/// `CSI O` as the terminal gains and loses focus.
const FOCUS_EVENT: ffi::Mode = ffi::mode(1004, false);

/// What the callbacks collect while the library parses, and what they
/// answer from. It lives on the heap so that its address, which the
/// library holds as userdata, stays the same while the terminal moves.
struct Sink {
    replies: Vec<u8>,
    signals: Vec<Signal>,
    /// "Pinrail <version>", for XTVERSION
    name: Vec<u8>,
    color_scheme: Option<ColorScheme>,
}

/// One terminal's screen and history.
pub struct Terminal {
    raw: ffi::Terminal,
    sink: Box<Sink>,
    /// whether focus reporting was on after the last write
    focus_reporting: bool,
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
        let terminal = Self::wrap(raw, options, false)?;
        terminal.set_size(ffi::OPT_CONTINUATION_MAX_BYTES, CONTINUATION_MAX_BYTES)?;
        Ok(terminal)
    }

    /// A terminal restored from a binary snapshot, with history limited to
    /// `scrollback_lines` as in [`Options`]. Its unfinished escape sequence,
    /// if any, is kept, so the next write completes it.
    pub fn restore(bytes: &[u8], scrollback_lines: usize) -> Result<Self, Error> {
        let mut decoder = ptr::null_mut();
        // SAFETY: the pointer and length describe `bytes`, which the
        // decoder reads during the calls below, before `bytes` goes away.
        check("ghostty_snapshot_decoder_new_buf", unsafe {
            ffi::ghostty_snapshot_decoder_new_buf(
                ptr::null(),
                &mut decoder,
                bytes.as_ptr(),
                bytes.len(),
            )
        })?;
        let (retain, limit) = (true, CONTINUATION_MAX_BYTES);
        let mut raw = ptr::null_mut();
        // SAFETY: the options take a bool* and a size_t*, read during the
        // call; decode hands over a new terminal or leaves null; the
        // decoder is freed once.
        let decoded = unsafe {
            ffi::ghostty_snapshot_decoder_set(
                decoder,
                ffi::DECODER_OPT_RETAIN_CONTINUATION,
                (&raw const retain).cast(),
            );
            ffi::ghostty_snapshot_decoder_set(
                decoder,
                ffi::DECODER_OPT_MAX_CONTINUATION_BYTES,
                (&raw const limit).cast(),
            );
            let decoded = ffi::ghostty_snapshot_decoder_decode(decoder, &mut raw);
            ffi::ghostty_snapshot_decoder_free(decoder);
            decoded
        };
        check("ghostty_snapshot_decoder_decode", decoded)?;
        if raw.is_null() {
            return Err(Error {
                call: "ghostty_snapshot_decoder_decode",
                code: ffi::INVALID_VALUE,
            });
        }
        let (cols, rows) = Terminal::size_of(raw);
        Self::wrap(
            raw,
            &Options {
                cols,
                rows,
                scrollback_lines,
            },
            true,
        )
    }

    /// Takes ownership of a terminal handle and sets what every terminal
    /// needs: the callbacks, the history limits and grapheme clustering.
    /// A `restored` terminal keeps the modes its program set.
    fn wrap(raw: ffi::Terminal, options: &Options, restored: bool) -> Result<Self, Error> {
        // from here on, Drop frees the handle if a setting fails
        let mut terminal = Terminal {
            raw,
            sink: Box::new(Sink {
                replies: Vec::new(),
                signals: Vec::new(),
                name: format!("Pinrail {}", env!("CARGO_PKG_VERSION")).into_bytes(),
                color_scheme: None,
            }),
            focus_reporting: false,
        };
        terminal.set_callbacks()?;
        terminal.set_size(ffi::OPT_SCROLLBACK_MAX_LINES, options.scrollback_lines())?;
        terminal.set_size(ffi::OPT_SCROLLBACK_MAX_BYTES, options.scrollback_bytes())?;
        // On as the reset default, so that a reset by the program keeps it.
        // Setting the default also sets the mode now, so a restored terminal
        // whose program had turned it off gets it turned off again.
        let was_on = terminal.mode_set(GRAPHEME_CLUSTER);
        terminal.set_mode(ffi::OPT_MODE_DEFAULT, GRAPHEME_CLUSTER, true)?;
        if restored && !was_on {
            terminal.set_mode(ffi::OPT_MODE, GRAPHEME_CLUSTER, false)?;
        }
        terminal.focus_reporting = terminal.mode_set(FOCUS_EVENT);
        Ok(terminal)
    }

    /// Sets a mode's value now (`OPT_MODE`) or as the reset default
    /// (`OPT_MODE_DEFAULT`).
    fn set_mode(
        &self,
        option: ffi::TerminalOption,
        mode: ffi::Mode,
        value: bool,
    ) -> Result<(), Error> {
        let config = ffi::ModeConfig { mode, value };
        // SAFETY: both options take a pointer to a GhosttyTerminalModeConfig,
        // read during the call.
        check("ghostty_terminal_set(MODE)", unsafe {
            ffi::ghostty_terminal_set(self.raw, option, (&raw const config).cast())
        })
    }

    /// Gives the library the sink as userdata, and the callbacks that fill
    /// it. Every callback runs on this thread, inside a write.
    fn set_callbacks(&self) -> Result<(), Error> {
        let sink: *const Sink = &*self.sink;
        let callbacks: [(ffi::TerminalOption, *const c_void); 8] = [
            (
                ffi::OPT_WRITE_PTY,
                on_write_pty as ffi::WritePtyFn as *const c_void,
            ),
            (ffi::OPT_BELL, on_bell as ffi::BellFn as *const c_void),
            (
                ffi::OPT_XTVERSION,
                on_xtversion as ffi::XtversionFn as *const c_void,
            ),
            (
                ffi::OPT_TITLE_CHANGED,
                on_title_changed as ffi::TitleChangedFn as *const c_void,
            ),
            (ffi::OPT_SIZE, on_size as ffi::SizeFn as *const c_void),
            (
                ffi::OPT_COLOR_SCHEME,
                on_color_scheme as ffi::ColorSchemeFn as *const c_void,
            ),
            (
                ffi::OPT_DESKTOP_NOTIFICATION,
                on_desktop_notification as ffi::DesktopNotificationFn as *const c_void,
            ),
            (
                ffi::OPT_PROGRAM_STATUS,
                on_program_status as ffi::ProgramStatusFn as *const c_void,
            ),
        ];
        // SAFETY: USERDATA takes the pointer itself, which stays valid as
        // long as the terminal, since the box is dropped after the handle
        // is freed.
        check("ghostty_terminal_set(USERDATA)", unsafe {
            ffi::ghostty_terminal_set(self.raw, ffi::OPT_USERDATA, sink.cast())
        })?;
        for (option, callback) in callbacks {
            // SAFETY: each option takes a function pointer of the type it
            // was cast from, which matches the header's typedef.
            check("ghostty_terminal_set(callback)", unsafe {
                ffi::ghostty_terminal_set(self.raw, option, callback)
            })?;
        }
        Ok(())
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

    /// Parses program output into the screen, ignoring what it answers
    /// or reports.
    pub fn write(&mut self, bytes: &[u8]) {
        let (mut replies, mut signals) = (Vec::new(), Vec::new());
        self.feed(bytes, &mut replies, &mut signals);
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

    fn size_of(raw: ffi::Terminal) -> (u16, u16) {
        let (mut cols, mut rows) = (0u16, 0u16);
        // SAFETY: both data items have a uint16_t as their output type.
        unsafe {
            ffi::ghostty_terminal_get(raw, ffi::DATA_COLS, (&raw mut cols).cast());
            ffi::ghostty_terminal_get(raw, ffi::DATA_ROWS, (&raw mut rows).cast());
        }
        (cols, rows)
    }

    /// Sets a default colour, or clears it with `None`.
    fn set_color(&self, option: ffi::TerminalOption, color: Option<Rgb>) {
        let rgb = color.map(|c| ffi::ColorRgb {
            r: c.r,
            g: c.g,
            b: c.b,
        });
        let value = match &rgb {
            Some(rgb) => (rgb as *const ffi::ColorRgb).cast(),
            None => ptr::null(),
        };
        // SAFETY: the colour options take a GhosttyColorRgb*, read during the
        // call, or null to clear the colour.
        unsafe { ffi::ghostty_terminal_set(self.raw, option, value) };
    }

    /// Whether the program shows the alternate screen.
    pub fn alternate_screen(&self) -> bool {
        self.get::<i32>(ffi::DATA_ACTIVE_SCREEN) == ffi::SCREEN_ALTERNATE
    }

    fn plain(&self, selection: Option<&ffi::Selection>) -> String {
        let bytes = self.format(ffi::FORMAT_PLAIN, selection, ffi::EXTRAS_NONE, false);
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// A reference to the cell at a point, if the point exists.
    fn grid_ref(&self, tag: i32, x: u16, y: u32) -> Option<ffi::GridRef> {
        let point = ffi::Point {
            tag,
            value: ffi::PointValue {
                coordinate: ffi::PointCoordinate { x, y },
            },
        };
        let mut out = ffi::GridRef {
            node: ptr::null_mut(),
            x: 0,
            y: 0,
        };
        // SAFETY: the point is passed by value, and `out` is a valid place
        // for the reference, which stays valid until the terminal changes.
        let found = unsafe { ffi::ghostty_terminal_grid_ref(self.raw, point, &mut out) };
        (found == ffi::SUCCESS).then_some(out)
    }

    /// The cells from `start` to `end`, both included, in one coordinate
    /// space.
    fn select(&self, tag: i32, start: (u16, u32), end: (u16, u32)) -> Option<ffi::Selection> {
        Some(ffi::Selection {
            start: self.grid_ref(tag, start.0, start.1)?,
            end: self.grid_ref(tag, end.0, end.1)?,
            rectangle: false,
        })
    }

    /// The selection (or the active screen) as plain text or VT sequences,
    /// with `extras`. `trimmed` joins wrapped lines and trims their ends.
    fn format(
        &self,
        emit: i32,
        selection: Option<&ffi::Selection>,
        extras: u32,
        trimmed: bool,
    ) -> Vec<u8> {
        let mut formatter = ptr::null_mut();
        // SAFETY: a null allocator selects the default; the formatter
        // borrows the terminal, which outlives it, as it is freed below.
        if unsafe { ffi::ghostty_formatter_terminal_new(ptr::null(), &mut formatter, self.raw) }
            != ffi::SUCCESS
        {
            return Vec::new();
        }
        let joined = trimmed || emit == ffi::FORMAT_PLAIN;
        // SAFETY: each option takes a pointer to a value of the type passed,
        // read during the call; the formatter copies the selection.
        unsafe {
            ffi::ghostty_formatter_set(
                formatter,
                ffi::FORMATTER_OPT_EMIT,
                (&raw const emit).cast(),
            );
            ffi::ghostty_formatter_set(
                formatter,
                ffi::FORMATTER_OPT_UNWRAP,
                (&raw const joined).cast(),
            );
            ffi::ghostty_formatter_set(
                formatter,
                ffi::FORMATTER_OPT_TRIM,
                (&raw const joined).cast(),
            );
            ffi::ghostty_formatter_set(
                formatter,
                ffi::FORMATTER_OPT_EXTRAS,
                (&raw const extras).cast(),
            );
            if let Some(selection) = selection {
                ffi::ghostty_formatter_set(
                    formatter,
                    ffi::FORMATTER_OPT_SELECTION,
                    (selection as *const ffi::Selection).cast(),
                );
            }
        }
        // SAFETY: format_alloc hands over the output; the formatter is
        // freed once.
        let bytes = alloc(|out, len| unsafe {
            ffi::ghostty_formatter_format_alloc(formatter, ptr::null(), out, len)
        });
        unsafe { ffi::ghostty_formatter_free(formatter) };
        bytes.unwrap_or_default()
    }

    /// The cell a grid reference points to.
    fn cell(&self, at: &ffi::GridRef) -> Cell {
        let mut raw: ffi::Cell = 0;
        // SAFETY: `at` came from grid_ref and the terminal has not changed
        // since; `raw` is a valid place for the cell.
        if unsafe { ffi::ghostty_grid_ref_cell(at, &mut raw) } != ffi::SUCCESS {
            return Cell::default();
        }
        let get = |data: i32, out: *mut c_void| {
            // SAFETY: each data item read here has the output type `out`
            // points to.
            unsafe { ffi::ghostty_cell_get(raw, data, out) }
        };
        let (mut wide, mut content, mut linked) = (0i32, 0i32, false);
        get(ffi::CELL_DATA_WIDE, (&raw mut wide).cast());
        get(ffi::CELL_DATA_CONTENT_TAG, (&raw mut content).cast());
        get(ffi::CELL_DATA_HAS_HYPERLINK, (&raw mut linked).cast());
        let mut style = convert_style(&self.raw_style(at));
        // a blank cell may hold only a background colour, outside any style
        match content {
            ffi::CELL_CONTENT_BG_COLOR_PALETTE => {
                let mut index = 0u8;
                get(ffi::CELL_DATA_COLOR_PALETTE, (&raw mut index).cast());
                style.background = Color::Palette(index);
            }
            ffi::CELL_CONTENT_BG_COLOR_RGB => {
                let mut rgb = ffi::ColorRgb::default();
                get(ffi::CELL_DATA_COLOR_RGB, (&raw mut rgb).cast());
                style.background = Color::Rgb(Rgb {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                });
            }
            _ => {}
        }
        Cell {
            text: graphemes(at),
            width: match wide {
                ffi::CELL_WIDE => 2,
                ffi::CELL_SPACER_TAIL => 0,
                _ => 1,
            },
            style,
            hyperlink: linked.then(|| hyperlink(at)).flatten(),
        }
    }

    fn raw_style(&self, at: &ffi::GridRef) -> ffi::Style {
        let mut style = ffi::Style {
            fg_color: NO_COLOR,
            bg_color: NO_COLOR,
            underline_color: NO_COLOR,
            flags: 0,
            reserved: 0,
        };
        // SAFETY: `at` is valid as in `cell`; `style` is a valid place for
        // the style, left as it is if the call fails.
        unsafe { ffi::ghostty_grid_ref_style(at, &mut style) };
        style
    }

    /// Whether a mode is set, by its number, for an ANSI mode or a DEC
    /// private one.
    pub fn mode(&self, number: u16, ansi: bool) -> bool {
        self.mode_set(ffi::mode(number, ansi))
    }

    fn mode_set(&self, mode: ffi::Mode) -> bool {
        let mut config = ffi::ModeConfig { mode, value: false };
        // SAFETY: DATA_MODE reads and writes a GhosttyTerminalModeConfig
        // whose `mode` is set first. An unknown mode fails and leaves false.
        unsafe { ffi::ghostty_terminal_get(self.raw, ffi::DATA_MODE, (&raw mut config).cast()) };
        config.value
    }

    /// The first of `modes` that is set, by DEC private mode number.
    fn first_set<T: Copy + Default>(&self, modes: &[(u16, T)]) -> T {
        modes
            .iter()
            .find(|(number, _)| self.mode(*number, false))
            .map(|(_, value)| *value)
            .unwrap_or_default()
    }

    /// The bytes of an escape sequence or UTF-8 character that the last
    /// write left unfinished: empty at ground, and `None` when it is
    /// unavailable, as after a sequence longer than the continuation holds.
    pub fn continuation(&self) -> Option<Vec<u8>> {
        // SAFETY: continuation_alloc hands over the unfinished bytes.
        alloc(|out, len| unsafe {
            ffi::ghostty_terminal_continuation_alloc(self.raw, ptr::null(), out, len)
        })
    }
}

impl Screen for Terminal {
    fn feed(&mut self, bytes: &[u8], replies: &mut Vec<u8>, signals: &mut Vec<Signal>) {
        // SAFETY: the pointer and length describe `bytes`, which the library
        // reads during the call only. The callbacks it makes meanwhile reach
        // the sink, which nothing else touches during the call.
        unsafe { ffi::ghostty_terminal_vt_write(self.raw, bytes.as_ptr(), bytes.len()) }
        replies.append(&mut self.sink.replies);
        signals.append(&mut self.sink.signals);
        let focus_reporting = self.mode_set(FOCUS_EVENT);
        if focus_reporting != self.focus_reporting {
            self.focus_reporting = focus_reporting;
            signals.push(Signal::FocusReporting(focus_reporting));
        }
    }

    fn resize(&mut self, cols: u16, rows: u16, cell_px: Option<(u16, u16)>) {
        if cols == 0 || rows == 0 {
            return;
        }
        let (width, height) = match cell_px {
            Some((w, h)) => (u32::from(w), u32::from(h)),
            None => self.cell_px(),
        };
        // SAFETY: the handle is valid, and the sizes are nonzero. The call
        // fails only when out of memory, which leaves the old size.
        unsafe { ffi::ghostty_terminal_resize(self.raw, cols, rows, width, height) };
    }

    fn set_facts(&mut self, facts: &DisplayFacts) {
        self.sink.color_scheme = facts.color_scheme;
        self.set_color(ffi::OPT_COLOR_FOREGROUND, facts.foreground);
        self.set_color(ffi::OPT_COLOR_BACKGROUND, facts.background);
        self.set_color(ffi::OPT_COLOR_CURSOR, facts.cursor);
    }

    fn first_paint(&self, history_lines: usize) -> Vec<u8> {
        self.paint(history_lines)
    }

    fn memory_bytes(&self) -> u64 {
        let mut usage = ffi::MemoryUsage {
            size: std::mem::size_of::<ffi::MemoryUsage>(),
            ..ffi::MemoryUsage::default()
        };
        // SAFETY: DATA_MEMORY_USAGE writes a GhosttyTerminalMemoryUsage,
        // whose size is set first; a failed read leaves zeros.
        unsafe {
            ffi::ghostty_terminal_get(self.raw, ffi::DATA_MEMORY_USAGE, (&raw mut usage).cast())
        };
        usage.primary_resident_bytes + usage.alternate_resident_bytes
    }

    fn paintable(&self) -> bool {
        // the continuation is unavailable once the sequence outgrew it
        self.get::<bool>(ffi::DATA_VT_GROUND) || self.continuation().is_some()
    }

    /// libghostty-vt's binary snapshot of the whole terminal: both screens,
    /// all modes and the whole history, and an unfinished escape sequence.
    fn binary_snapshot(&self) -> Option<BinarySnapshot> {
        // SAFETY: encode_alloc hands over the encoded bytes.
        let bytes = alloc(|out, len| unsafe {
            ffi::ghostty_snapshot_encode_alloc(self.raw, ptr::null(), out, len)
        })?;
        Some(BinarySnapshot {
            format: format!("ghostty-bin;vt-{}", version()),
            bytes,
        })
    }

    fn text(&self, range: TextRange) -> String {
        let (cols, rows) = self.size();
        let selection = match range {
            TextRange::Screen => {
                self.select(ffi::POINT_ACTIVE, (0, 0), (cols - 1, u32::from(rows) - 1))
            }
            TextRange::History => match self.retained_lines() as u32 {
                0 => None,
                lines => self.select(ffi::POINT_HISTORY, (0, 0), (cols - 1, lines - 1)),
            },
        };
        match selection {
            Some(selection) => self.plain(Some(&selection)),
            None => String::new(),
        }
    }

    fn modes(&self) -> InputModes {
        InputModes {
            application_cursor: self.mode(1, false),
            application_keypad: self.mode(66, false),
            bracketed_paste: self.mode(2004, false),
            focus_reporting: self.mode_set(FOCUS_EVENT),
            mouse_tracking: self.first_set(&[
                (1003, MouseTracking::Any),
                (1002, MouseTracking::Button),
                (1000, MouseTracking::Normal),
                (9, MouseTracking::X10),
            ]),
            mouse_format: self.first_set(&[
                (1016, MouseFormat::SgrPixels),
                (1006, MouseFormat::Sgr),
                (1015, MouseFormat::Urxvt),
                (1005, MouseFormat::Utf8),
            ]),
            kitty_keyboard: self.get(ffi::DATA_KITTY_KEYBOARD_FLAGS),
            alternate_screen: self.alternate_screen(),
        }
    }

    fn title(&self) -> Option<String> {
        let mut title = ffi::GhosttyString {
            ptr: ptr::null(),
            len: 0,
        };
        // SAFETY: DATA_TITLE's output type is a GhosttyString, which stays
        // valid until the terminal next changes; it is copied at once.
        unsafe { ffi::ghostty_terminal_get(self.raw, ffi::DATA_TITLE, (&raw mut title).cast()) };
        Some(text(title)).filter(|title| !title.is_empty())
    }

    fn retained_lines(&self) -> usize {
        self.get(ffi::DATA_SCROLLBACK_ROWS)
    }

    fn size(&self) -> (u16, u16) {
        Self::size_of(self.raw)
    }

    fn cursor(&self) -> Cursor {
        Cursor {
            x: self.get(ffi::DATA_CURSOR_X),
            y: self.get(ffi::DATA_CURSOR_Y),
            visible: self.get(ffi::DATA_CURSOR_VISIBLE),
        }
    }

    fn row(&self, y: u16) -> Vec<Cell> {
        let (cols, _) = self.size();
        (0..cols)
            .map_while(|x| self.grid_ref(ffi::POINT_ACTIVE, x, u32::from(y)))
            .map(|at| self.cell(&at))
            .collect()
    }
}

const NO_COLOR: ffi::StyleColor = ffi::StyleColor {
    tag: 0,
    value: ffi::StyleColorValue { palette: 0 },
};

fn convert_color(color: &ffi::StyleColor) -> Color {
    // SAFETY: the tag says which field of the union the library wrote.
    unsafe {
        match color.tag {
            ffi::STYLE_COLOR_PALETTE => Color::Palette(color.value.palette),
            ffi::STYLE_COLOR_RGB => {
                let rgb = color.value.rgb;
                Color::Rgb(Rgb {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                })
            }
            _ => Color::Default,
        }
    }
}

fn convert_style(style: &ffi::Style) -> Style {
    let flag = |bit: u16| style.flags & bit != 0;
    Style {
        foreground: convert_color(&style.fg_color),
        background: convert_color(&style.bg_color),
        underline_color: convert_color(&style.underline_color),
        underline: match (style.flags & ffi::STYLE_UNDERLINE_MASK) >> ffi::STYLE_UNDERLINE_SHIFT {
            0 => Underline::None,
            2 => Underline::Double,
            3 => Underline::Curly,
            4 => Underline::Dotted,
            5 => Underline::Dashed,
            _ => Underline::Single,
        },
        bold: flag(ffi::STYLE_BOLD),
        italic: flag(ffi::STYLE_ITALIC),
        faint: flag(ffi::STYLE_FAINT),
        blink: flag(ffi::STYLE_BLINK),
        inverse: flag(ffi::STYLE_INVERSE),
        invisible: flag(ffi::STYLE_INVISIBLE),
        strikethrough: flag(ffi::STYLE_STRIKETHROUGH),
        overline: flag(ffi::STYLE_OVERLINE),
    }
}

/// The grapheme cluster of the cell a grid reference points to.
fn graphemes(at: &ffi::GridRef) -> String {
    let mut buf = vec![0u32; 8];
    let mut written = 0usize;
    // SAFETY: `buf` holds `buf.len()` codepoints, and `written` is a valid
    // place for the count; a buffer too small is left unwritten.
    let mut result =
        unsafe { ffi::ghostty_grid_ref_graphemes(at, buf.as_mut_ptr(), buf.len(), &mut written) };
    if result == ffi::OUT_OF_SPACE {
        buf.resize(written, 0);
        // SAFETY: as above, with a buffer of the size the library asked for.
        result = unsafe {
            ffi::ghostty_grid_ref_graphemes(at, buf.as_mut_ptr(), buf.len(), &mut written)
        };
    }
    if result != ffi::SUCCESS {
        return String::new();
    }
    buf[..written.min(buf.len())]
        .iter()
        .filter_map(|&c| char::from_u32(c))
        .collect()
}

/// The hyperlink URI of the cell a grid reference points to.
fn hyperlink(at: &ffi::GridRef) -> Option<String> {
    let mut len = 0usize;
    // SAFETY: a null buffer of length 0 asks for the size only.
    let result = unsafe { ffi::ghostty_grid_ref_hyperlink_uri(at, ptr::null_mut(), 0, &mut len) };
    if result != ffi::OUT_OF_SPACE {
        return None;
    }
    let mut buf = vec![0u8; len];
    // SAFETY: `buf` holds the `len` bytes the library asked for.
    let result =
        unsafe { ffi::ghostty_grid_ref_hyperlink_uri(at, buf.as_mut_ptr(), len, &mut len) };
    (result == ffi::SUCCESS).then(|| String::from_utf8_lossy(&buf[..len]).into_owned())
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // SAFETY: the handle came from ghostty_terminal_new and is freed once.
        unsafe { ffi::ghostty_terminal_free(self.raw) }
    }
}

/// Bytes the library allocates and hands over: copied, then freed with the
/// default allocator, which `call` must have allocated them with. `None`
/// when the call fails.
fn alloc(call: impl FnOnce(*mut *mut u8, *mut usize) -> ffi::GhosttyResult) -> Option<Vec<u8>> {
    let (mut out, mut len) = (ptr::null_mut(), 0usize);
    if call(&mut out, &mut len) != ffi::SUCCESS {
        return None;
    }
    if out.is_null() {
        return Some(Vec::new());
    }
    // SAFETY: on success the library hands over `len` bytes at `out`,
    // allocated with the default allocator, which are copied and freed once.
    unsafe {
        let bytes = std::slice::from_raw_parts(out, len).to_vec();
        ffi::ghostty_free(ptr::null(), out, len);
        Some(bytes)
    }
}

/// The sink a callback's userdata points to.
///
/// # Safety
///
/// `userdata` is the pointer `set_callbacks` gave, to the terminal's live
/// sink, and the callback runs inside a write that holds the terminal
/// mutably, so no other reference to the sink exists meanwhile.
unsafe fn sink<'a>(userdata: *mut c_void) -> &'a mut Sink {
    // SAFETY: as the function's contract states.
    unsafe { &mut *userdata.cast::<Sink>() }
}

/// Borrowed text from the library, copied.
fn text(s: ffi::GhosttyString) -> String {
    if s.ptr.is_null() || s.len == 0 {
        return String::new();
    }
    // SAFETY: the library passes `len` valid bytes at `ptr`, borrowed for
    // the duration of the callback, during which they are copied.
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(s.ptr, s.len) }).into_owned()
}

extern "C" fn on_write_pty(_: ffi::Terminal, userdata: *mut c_void, data: *const u8, len: usize) {
    if data.is_null() {
        return;
    }
    // SAFETY: see `sink`; the library passes `len` valid bytes at `data`.
    unsafe {
        sink(userdata)
            .replies
            .extend_from_slice(std::slice::from_raw_parts(data, len))
    }
}

extern "C" fn on_bell(_: ffi::Terminal, userdata: *mut c_void) {
    // SAFETY: see `sink`.
    unsafe { sink(userdata).signals.push(Signal::Bell) }
}

extern "C" fn on_xtversion(_: ffi::Terminal, userdata: *mut c_void) -> ffi::GhosttyString {
    // SAFETY: see `sink`. The name lives in the sink, longer than the call.
    let name = unsafe { &sink(userdata).name };
    ffi::GhosttyString {
        ptr: name.as_ptr(),
        len: name.len(),
    }
}

extern "C" fn on_title_changed(terminal: ffi::Terminal, userdata: *mut c_void) {
    let mut title = ffi::GhosttyString {
        ptr: ptr::null(),
        len: 0,
    };
    // SAFETY: the header says to read DATA_TITLE inside this callback; its
    // output type is a GhosttyString, valid during the callback.
    unsafe { ffi::ghostty_terminal_get(terminal, ffi::DATA_TITLE, (&raw mut title).cast()) };
    let title = text(title);
    // SAFETY: see `sink`.
    unsafe { sink(userdata).signals.push(Signal::Title(title)) }
}

extern "C" fn on_size(
    terminal: ffi::Terminal,
    _: *mut c_void,
    out: *mut ffi::SizeReportSize,
) -> bool {
    if out.is_null() {
        return false;
    }
    let get = |data, out: *mut c_void| {
        // SAFETY: each data item read here has the output type `out`
        // points to.
        unsafe { ffi::ghostty_terminal_get(terminal, data, out) };
    };
    let (mut cols, mut rows, mut width, mut height) = (0u16, 0u16, 0u32, 0u32);
    get(ffi::DATA_COLS, (&raw mut cols).cast());
    get(ffi::DATA_ROWS, (&raw mut rows).cast());
    get(ffi::DATA_WIDTH_PX, (&raw mut width).cast());
    get(ffi::DATA_HEIGHT_PX, (&raw mut height).cast());
    // SAFETY: `out` is the library's zeroed struct, writable during the call.
    unsafe {
        *out = ffi::SizeReportSize {
            rows,
            columns: cols,
            cell_width: width / u32::from(cols.max(1)),
            cell_height: height / u32::from(rows.max(1)),
        };
    }
    true
}

extern "C" fn on_color_scheme(_: ffi::Terminal, userdata: *mut c_void, out: *mut i32) -> bool {
    // SAFETY: see `sink`.
    let Some(scheme) = (unsafe { sink(userdata).color_scheme }) else {
        return false;
    };
    if out.is_null() {
        return false;
    }
    // SAFETY: `out` is the library's value, writable during the call.
    unsafe {
        *out = match scheme {
            ColorScheme::Light => ffi::COLOR_SCHEME_LIGHT,
            ColorScheme::Dark => ffi::COLOR_SCHEME_DARK,
        };
    }
    true
}

extern "C" fn on_desktop_notification(
    _: ffi::Terminal,
    userdata: *mut c_void,
    notification: *const ffi::DesktopNotification,
) {
    // SAFETY: the library passes a valid notification for the call.
    let Some(notification) = (unsafe { notification.as_ref() }) else {
        return;
    };
    let title = text(notification.title);
    let signal = Signal::Notify {
        title: (!title.is_empty()).then_some(title),
        body: text(notification.body),
    };
    // SAFETY: see `sink`.
    unsafe { sink(userdata).signals.push(signal) }
}

extern "C" fn on_program_status(
    _: ffi::Terminal,
    userdata: *mut c_void,
    report: *const ffi::ProgramStatus,
) {
    // SAFETY: the library passes a valid report for the call.
    let Some(report) = (unsafe { report.as_ref() }) else {
        return;
    };
    let state = match report.state {
        ffi::PROGRAM_STATUS_IDLE => ProgramState::Idle,
        ffi::PROGRAM_STATUS_WORKING => ProgramState::Working,
        ffi::PROGRAM_STATUS_DONE => ProgramState::Done,
        ffi::PROGRAM_STATUS_BLOCKED => ProgramState::Blocked,
        ffi::PROGRAM_STATUS_ERROR => ProgramState::Error,
        ffi::PROGRAM_STATUS_CLEAR => ProgramState::Clear,
        // a state a later version adds
        _ => return,
    };
    let wait = match report.kind {
        ffi::PROGRAM_STATUS_KIND_PERMISSION => Some(ProgramWait::Permission),
        ffi::PROGRAM_STATUS_KIND_QUESTION => Some(ProgramWait::Question),
        ffi::PROGRAM_STATUS_KIND_AUTH => Some(ProgramWait::Auth),
        _ => None,
    };
    let status = ProgramStatus {
        state,
        wait,
        progress: u8::try_from(report.progress).ok(),
        id: text(report.id),
        app: text(report.app),
        title: text(report.title),
        message: text(report.message),
    };
    // SAFETY: see `sink`.
    unsafe { sink(userdata).signals.push(Signal::ProgramStatus(status)) }
}
