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
pub const OUT_OF_SPACE: GhosttyResult = -3;

/// `GhosttyTerminalOption` (terminal.h).
pub type TerminalOption = i32;
pub const OPT_USERDATA: TerminalOption = 1;
pub const OPT_WRITE_PTY: TerminalOption = 2;
pub const OPT_BELL: TerminalOption = 3;
pub const OPT_XTVERSION: TerminalOption = 5;
pub const OPT_TITLE_CHANGED: TerminalOption = 6;
pub const OPT_SIZE: TerminalOption = 7;
pub const OPT_COLOR_SCHEME: TerminalOption = 8;
pub const OPT_COLOR_FOREGROUND: TerminalOption = 12;
pub const OPT_COLOR_BACKGROUND: TerminalOption = 13;
pub const OPT_COLOR_CURSOR: TerminalOption = 14;
pub const OPT_DESKTOP_NOTIFICATION: TerminalOption = 30;
pub const OPT_PROGRAM_STATUS: TerminalOption = 47;
pub const OPT_SCROLLBACK_MAX_BYTES: TerminalOption = 28;
pub const OPT_SCROLLBACK_MAX_LINES: TerminalOption = 29;
pub const OPT_CONTINUATION_MAX_BYTES: TerminalOption = 32;
pub const OPT_MODE_DEFAULT: TerminalOption = 34;
pub const OPT_MODE: TerminalOption = 35;

/// `GhosttyTerminalData` (terminal.h).
pub type TerminalData = i32;
pub const DATA_COLS: TerminalData = 1;
pub const DATA_ROWS: TerminalData = 2;
pub const DATA_CURSOR_X: TerminalData = 3;
pub const DATA_CURSOR_Y: TerminalData = 4;
pub const DATA_CURSOR_VISIBLE: TerminalData = 7;
pub const DATA_KITTY_KEYBOARD_FLAGS: TerminalData = 8;
pub const DATA_SCROLLBACK_ROWS: TerminalData = 15;
pub const DATA_WIDTH_PX: TerminalData = 16;
pub const DATA_HEIGHT_PX: TerminalData = 17;
pub const DATA_ACTIVE_SCREEN: TerminalData = 6;
pub const DATA_TITLE: TerminalData = 12;
pub const DATA_MODE: TerminalData = 37;
pub const DATA_VT_GROUND: TerminalData = 38;

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

/// `GhosttyTerminalScreen` (terminal.h).
pub const SCREEN_ALTERNATE: i32 = 1;

/// An opaque formatter handle (`GhosttyFormatter`).
pub type Formatter = *mut c_void;
/// An opaque snapshot decoder handle (`GhosttySnapshotDecoder`).
pub type SnapshotDecoder = *mut c_void;

/// `GhosttyFormatterOption` (formatter.h).
pub const FORMATTER_OPT_EMIT: i32 = 1;
pub const FORMATTER_OPT_UNWRAP: i32 = 2;
pub const FORMATTER_OPT_TRIM: i32 = 3;
pub const FORMATTER_OPT_SELECTION: i32 = 4;
pub const FORMATTER_OPT_EXTRAS: i32 = 5;
/// `GhosttyFormatterFormat` (types.h).
pub const FORMAT_PLAIN: i32 = 0;
pub const FORMAT_VT: i32 = 1;
/// `GhosttyFormatterExtras` (formatter.h): no extras, or every one.
pub const EXTRAS_NONE: u32 = 0;
pub const EXTRAS_ALL: u32 = 0xFFF;
pub const EXTRA_PALETTE: u32 = 1;

/// `GhosttySnapshotDecoderOption` (snapshot.h).
pub const DECODER_OPT_MAX_CONTINUATION_BYTES: i32 = 1;
pub const DECODER_OPT_RETAIN_CONTINUATION: i32 = 2;

/// `GhosttyGridRef` (grid_ref.h).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GridRef {
    pub node: *mut c_void,
    pub x: u16,
    pub y: u16,
}

/// `GhosttyPointTag` (point.h).
pub const POINT_ACTIVE: i32 = 0;
pub const POINT_HISTORY: i32 = 3;

/// `GhosttyPointCoordinate` (point.h).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PointCoordinate {
    pub x: u16,
    pub y: u32,
}

/// `GhosttyPointValue` (point.h).
#[repr(C)]
#[derive(Clone, Copy)]
pub union PointValue {
    pub coordinate: PointCoordinate,
    pub padding: [u64; 2],
}

/// `GhosttyPoint` (point.h).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Point {
    pub tag: i32,
    pub value: PointValue,
}

/// `GhosttySelection` (selection.h).
#[repr(C)]
pub struct Selection {
    pub start: GridRef,
    pub end: GridRef,
    pub rectangle: bool,
}

/// `GhosttyCell` (screen.h): a cell's packed value, read with
/// `ghostty_cell_get`.
pub type Cell = u64;

/// `GhosttyCellData` (screen.h).
pub const CELL_DATA_CONTENT_TAG: i32 = 2;
pub const CELL_DATA_WIDE: i32 = 3;
pub const CELL_DATA_HAS_HYPERLINK: i32 = 7;
pub const CELL_DATA_COLOR_PALETTE: i32 = 10;
pub const CELL_DATA_COLOR_RGB: i32 = 11;

/// `GhosttyCellContentTag` (screen.h): a cell holding only a background
/// colour.
pub const CELL_CONTENT_BG_COLOR_PALETTE: i32 = 2;
pub const CELL_CONTENT_BG_COLOR_RGB: i32 = 3;

/// `GhosttyCellWide` (screen.h).
pub const CELL_WIDE: i32 = 1;
pub const CELL_SPACER_TAIL: i32 = 2;

/// `GhosttyColorRgb` (color.h).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ColorRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// `GhosttyStyleColorTag` (style.h).
pub const STYLE_COLOR_PALETTE: u8 = 1;
pub const STYLE_COLOR_RGB: u8 = 2;

/// `GhosttyStyleColorValue` (style.h).
#[repr(C)]
#[derive(Clone, Copy)]
pub union StyleColorValue {
    pub palette: u8,
    pub rgb: ColorRgb,
}

/// `GhosttyStyleColor` (style.h).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StyleColor {
    pub tag: u8,
    pub value: StyleColorValue,
}

/// `GhosttyStyleFlags` (style.h): one bit per attribute, and the underline
/// style (`GhosttySgrUnderline`) in bits 8 to 10.
pub const STYLE_BOLD: u16 = 1 << 0;
pub const STYLE_ITALIC: u16 = 1 << 1;
pub const STYLE_FAINT: u16 = 1 << 2;
pub const STYLE_BLINK: u16 = 1 << 3;
pub const STYLE_INVERSE: u16 = 1 << 4;
pub const STYLE_INVISIBLE: u16 = 1 << 5;
pub const STYLE_STRIKETHROUGH: u16 = 1 << 6;
pub const STYLE_OVERLINE: u16 = 1 << 7;
pub const STYLE_UNDERLINE_SHIFT: u16 = 8;
pub const STYLE_UNDERLINE_MASK: u16 = 0x7 << STYLE_UNDERLINE_SHIFT;

/// `GhosttyStyle` (style.h).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Style {
    pub fg_color: StyleColor,
    pub bg_color: StyleColor,
    pub underline_color: StyleColor,
    pub flags: u16,
    pub reserved: u16,
}

/// `GhosttyString` (types.h): borrowed bytes, not NUL-terminated.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyString {
    pub ptr: *const u8,
    pub len: usize,
}

/// `GhosttySizeReportSize` (size_report.h).
#[repr(C)]
pub struct SizeReportSize {
    pub rows: u16,
    pub columns: u16,
    pub cell_width: u32,
    pub cell_height: u32,
}

/// `GhosttyColorScheme` (device.h).
pub const COLOR_SCHEME_LIGHT: i32 = 0;
pub const COLOR_SCHEME_DARK: i32 = 1;

/// `GhosttyTerminalDesktopNotification` (terminal.h), whose layout is
/// frozen.
#[repr(C)]
pub struct DesktopNotification {
    pub title: GhosttyString,
    pub body: GhosttyString,
}

/// `GhosttyProgramStatusState` (terminal.h).
pub const PROGRAM_STATUS_IDLE: i32 = 0;
pub const PROGRAM_STATUS_WORKING: i32 = 1;
pub const PROGRAM_STATUS_DONE: i32 = 2;
pub const PROGRAM_STATUS_BLOCKED: i32 = 3;
pub const PROGRAM_STATUS_ERROR: i32 = 4;
pub const PROGRAM_STATUS_CLEAR: i32 = 5;

/// `GhosttyProgramStatusKind` (terminal.h).
pub const PROGRAM_STATUS_KIND_PERMISSION: i32 = 1;
pub const PROGRAM_STATUS_KIND_QUESTION: i32 = 2;
pub const PROGRAM_STATUS_KIND_AUTH: i32 = 3;

/// `GhosttyTerminalProgramStatus` (terminal.h), whose layout is frozen.
#[repr(C)]
pub struct ProgramStatus {
    pub state: i32,
    pub kind: i32,
    /// 0 to 100, or -1 for none
    pub progress: i8,
    pub id: GhosttyString,
    pub app: GhosttyString,
    pub title: GhosttyString,
    pub message: GhosttyString,
}

pub type WritePtyFn = extern "C" fn(Terminal, *mut c_void, *const u8, usize);
pub type BellFn = extern "C" fn(Terminal, *mut c_void);
pub type XtversionFn = extern "C" fn(Terminal, *mut c_void) -> GhosttyString;
pub type TitleChangedFn = extern "C" fn(Terminal, *mut c_void);
pub type SizeFn = extern "C" fn(Terminal, *mut c_void, *mut SizeReportSize) -> bool;
pub type ColorSchemeFn = extern "C" fn(Terminal, *mut c_void, *mut i32) -> bool;
pub type DesktopNotificationFn = extern "C" fn(Terminal, *mut c_void, *const DesktopNotification);
pub type ProgramStatusFn = extern "C" fn(Terminal, *mut c_void, *const ProgramStatus);

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
    pub fn ghostty_terminal_grid_ref(
        terminal: Terminal,
        point: Point,
        out_ref: *mut GridRef,
    ) -> GhosttyResult;
    pub fn ghostty_grid_ref_cell(grid_ref: *const GridRef, out_cell: *mut Cell) -> GhosttyResult;
    pub fn ghostty_grid_ref_graphemes(
        grid_ref: *const GridRef,
        buf: *mut u32,
        buf_len: usize,
        out_written: *mut usize,
    ) -> GhosttyResult;
    pub fn ghostty_grid_ref_hyperlink_uri(
        grid_ref: *const GridRef,
        buf: *mut u8,
        buf_len: usize,
        out_written: *mut usize,
    ) -> GhosttyResult;
    pub fn ghostty_grid_ref_style(grid_ref: *const GridRef, out_style: *mut Style)
    -> GhosttyResult;
    pub fn ghostty_cell_get(cell: Cell, data: i32, out: *mut c_void) -> GhosttyResult;
    pub fn ghostty_formatter_terminal_new(
        allocator: *const c_void,
        formatter: *mut Formatter,
        terminal: Terminal,
    ) -> GhosttyResult;
    pub fn ghostty_formatter_set(
        formatter: Formatter,
        option: i32,
        value: *const c_void,
    ) -> GhosttyResult;
    pub fn ghostty_formatter_format_alloc(
        formatter: Formatter,
        allocator: *const c_void,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> GhosttyResult;
    pub fn ghostty_formatter_free(formatter: Formatter);
    pub fn ghostty_snapshot_encode_alloc(
        terminal: Terminal,
        allocator: *const c_void,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> GhosttyResult;
    pub fn ghostty_snapshot_decoder_new_buf(
        allocator: *const c_void,
        decoder: *mut SnapshotDecoder,
        ptr: *const u8,
        len: usize,
    ) -> GhosttyResult;
    pub fn ghostty_snapshot_decoder_set(
        decoder: SnapshotDecoder,
        option: i32,
        value: *const c_void,
    ) -> GhosttyResult;
    pub fn ghostty_snapshot_decoder_decode(
        decoder: SnapshotDecoder,
        terminal: *mut Terminal,
    ) -> GhosttyResult;
    pub fn ghostty_snapshot_decoder_free(decoder: SnapshotDecoder);
    pub fn ghostty_version() -> *const std::ffi::c_char;
}

// the sizes the headers give for the frozen structs on 64-bit targets
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(std::mem::size_of::<ProgramStatus>() == 80);
    assert!(std::mem::size_of::<DesktopNotification>() == 32);
    assert!(std::mem::size_of::<SizeReportSize>() == 12);
    assert!(std::mem::size_of::<GridRef>() == 16);
    assert!(std::mem::size_of::<Point>() == 24);
    assert!(std::mem::size_of::<Style>() == 16);
};
