//! The C API of libghostty-vt 1.0, declared from its headers
//! (`include/ghostty/vt/*.h` of the release `ghostty.lock` pins). Only what
//! the safe interface in the parent module calls is declared. Every enum of
//! the API is a C `int`.

use std::ffi::c_void;

/// An opaque terminal handle (`GhosttyTerminal`).
pub type Terminal = *mut c_void;

/// `GhosttyResult` (types.h).
pub type GhosttyResult = i32;
pub const SUCCESS: GhosttyResult = 0;
pub const OUT_OF_MEMORY: GhosttyResult = -1;
pub const INVALID_VALUE: GhosttyResult = -2;

/// `GhosttyTerminalOption` (terminal.h).
pub type TerminalOption = i32;
pub const OPT_SCROLLBACK_MAX_BYTES: TerminalOption = 28;
pub const OPT_SCROLLBACK_MAX_LINES: TerminalOption = 29;
pub const OPT_CONTINUATION_MAX_BYTES: TerminalOption = 32;
pub const OPT_MODE_DEFAULT: TerminalOption = 34;

/// `GhosttyTerminalData` (terminal.h).
pub type TerminalData = i32;
pub const DATA_COLS: TerminalData = 1;
pub const DATA_ROWS: TerminalData = 2;
pub const DATA_CURSOR_X: TerminalData = 3;
pub const DATA_CURSOR_Y: TerminalData = 4;
pub const DATA_SCROLLBACK_ROWS: TerminalData = 15;
pub const DATA_WIDTH_PX: TerminalData = 16;
pub const DATA_HEIGHT_PX: TerminalData = 17;
pub const DATA_MODE: TerminalData = 37;

/// `GhosttyMode` (modes.h): the mode's number in bits 0 to 14, and bit 15
/// set for an ANSI mode rather than a DEC private one.
pub type Mode = u16;

/// The packed mode, as `ghostty_mode_new` makes it.
pub const fn mode(value: u16, ansi: bool) -> Mode {
    (value & 0x7FFF) | ((ansi as u16) << 15)
}

/// `GhosttyTerminalModeConfig` (terminal.h), whose layout is frozen.
#[repr(C)]
pub struct ModeConfig {
    pub mode: Mode,
    pub value: bool,
}

unsafe extern "C" {
    pub fn ghostty_terminal_new(
        allocator: *const c_void,
        terminal: *mut Terminal,
        cols: u16,
        rows: u16,
    ) -> GhosttyResult;
    pub fn ghostty_terminal_free(terminal: Terminal);
    pub fn ghostty_terminal_resize(
        terminal: Terminal,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> GhosttyResult;
    pub fn ghostty_terminal_set(
        terminal: Terminal,
        option: TerminalOption,
        value: *const c_void,
    ) -> GhosttyResult;
    pub fn ghostty_terminal_get(
        terminal: Terminal,
        data: TerminalData,
        out: *mut c_void,
    ) -> GhosttyResult;
    pub fn ghostty_terminal_vt_write(terminal: Terminal, data: *const u8, len: usize);
    pub fn ghostty_terminal_continuation_alloc(
        terminal: Terminal,
        allocator: *const c_void,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> GhosttyResult;
    pub fn ghostty_free(allocator: *const c_void, ptr: *mut u8, len: usize);
    pub fn ghostty_version() -> *const std::ffi::c_char;
}
