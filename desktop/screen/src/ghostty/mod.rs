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

/// Focus reporting (DEC private mode 1004): the program wants `CSI I` and
/// `CSI O` as the terminal gains and loses focus.
const FOCUS_EVENT: ffi::Mode = ffi::mode(1004, false);

/// What the host's view of the terminal tells a program that asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
}

/// Something the program told its terminal while it was parsed, other than
/// screen content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    Title(String),
    Bell,
    /// A desktop notification (OSC 9 or OSC 777).
    Notify {
        title: Option<String>,
        body: String,
    },
    /// A program status report (OSC 7501).
    ProgramStatus(ProgramStatus),
    /// The program turned focus reporting (mode 1004) on or off.
    FocusReporting(bool),
}

/// A program status report (OSC 7501), as the Program Status Protocol
/// defines it. The text is the program's own, decoded, with no control
/// characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramStatus {
    pub state: ProgramState,
    /// What a blocked program waits for, when it says.
    pub wait: Option<ProgramWait>,
    /// How far along the work is, from 0 to 100, when the program says.
    pub progress: Option<u8>,
    /// The record it is about; empty for the program itself.
    pub id: String,
    pub app: String,
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramState {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
    /// Removes the record with this id and those beneath it; an empty id
    /// removes every record.
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramWait {
    Permission,
    Question,
    Auth,
}

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
        // from here on, Drop frees the handle if a setting fails
        let terminal = Terminal {
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

    /// Parses program output into the screen. The answers to the queries
    /// in it are appended to `replies`, in order, for the program; what it
    /// tells its terminal otherwise is appended to `signals`.
    pub fn feed(&mut self, bytes: &[u8], replies: &mut Vec<u8>, signals: &mut Vec<Signal>) {
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

    /// The colour scheme the viewer shows, for a program that asks; `None`
    /// leaves the question unanswered.
    pub fn set_color_scheme(&mut self, scheme: Option<ColorScheme>) {
        self.sink.color_scheme = scheme;
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
        self.mode_set(ffi::mode(number, ansi))
    }

    fn mode_set(&self, mode: ffi::Mode) -> bool {
        let mut config = ffi::ModeConfig { mode, value: false };
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
