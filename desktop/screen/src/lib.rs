//! The terminal screen of an agent's run.
//!
//! The host talks to a terminal only through the [`Screen`] trait, so the
//! emulator behind it can be replaced. The `ghostty` feature provides the
//! implementation on libghostty-vt, which `build.rs` builds from the
//! release `ghostty.lock` pins.

// the terminal host runs agents in PTYs, so only on Unix
#[cfg(all(feature = "ghostty", unix))]
pub mod ghostty;

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

/// One terminal's screen, history and parser, as the host's actor uses it.
pub trait Screen: Send {
    /// Parses program output into the screen. The answers to the queries in
    /// it are appended to `replies`, in order, for the program to read;
    /// what it tells its terminal otherwise is appended to `signals`.
    fn feed(&mut self, bytes: &[u8], replies: &mut Vec<u8>, signals: &mut Vec<Signal>);

    /// Changes the size in cells, and the cell size in pixels that image
    /// protocols and size reports use, which stays as it was when `None`.
    /// A size of zero is ignored.
    fn resize(&mut self, cols: u16, rows: u16, cell_px: Option<(u16, u16)>);

    /// Sets what the controlling viewer shows, for the colour queries a
    /// program sends.
    fn set_facts(&mut self, facts: &DisplayFacts);

    /// VT bytes that rebuild this screen in any VT emulator of the same
    /// size: up to `history_lines` lines of history, pushed into the
    /// viewer's own scrollback, the primary screen, and the alternate
    /// screen over it when the program uses one, with styles, cursor, modes
    /// and title, but not the palette, so the viewer's theme applies.
    /// History is best effort, as an implementation may prune it in blocks.
    fn first_paint(&self, history_lines: usize) -> Vec<u8>;

    /// An exact copy of the terminal for a viewer built on the same
    /// library build, or `None` when the implementation has none.
    fn binary_snapshot(&self) -> Option<BinarySnapshot>;

    /// The text of part of the terminal, a line per row, without styles.
    fn text(&self, range: TextRange) -> String;

    /// The modes that decide how the person's input is encoded.
    fn modes(&self) -> InputModes;

    /// The title the program set, if any.
    fn title(&self) -> Option<String>;

    /// The lines of history the terminal holds now.
    fn retained_lines(&self) -> usize;

    /// The size in cells, as columns and rows.
    fn size(&self) -> (u16, u16);

    /// The cursor on the active screen.
    fn cursor(&self) -> Cursor;

    /// The cells of a row of the active screen, counted from the top, or
    /// none when the row does not exist.
    fn row(&self, y: u16) -> Vec<Cell>;
}

/// How a terminal is created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub cols: u16,
    pub rows: u16,
    /// Lines of history to keep, at most [`MAX_SCROLLBACK_LINES`]. An
    /// implementation that prunes by block may keep somewhat fewer.
    pub scrollback_lines: usize,
}

impl Options {
    /// The line limit of the history, at most [`MAX_SCROLLBACK_LINES`].
    pub fn scrollback_lines(&self) -> usize {
        self.scrollback_lines.min(MAX_SCROLLBACK_LINES)
    }

    /// The byte limit of the history: [`SCROLLBACK_BYTES_PER_10K_LINES`]
    /// for every 10,000 lines of the line limit.
    pub fn scrollback_bytes(&self) -> usize {
        self.scrollback_lines() * SCROLLBACK_BYTES_PER_10K_LINES / 10_000
    }
}

/// An opaque copy of a terminal, for a viewer built on the same library
/// build, which `format` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinarySnapshot {
    /// Such as "ghostty-bin;vt-1.0.0"
    pub format: String,
    pub bytes: Vec<u8>,
}

/// What the controlling viewer shows, for a program that asks. A colour
/// that is `None` is the implementation's default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisplayFacts {
    /// `None` leaves a colour scheme query unanswered.
    pub color_scheme: Option<ColorScheme>,
    pub foreground: Option<Rgb>,
    pub background: Option<Rgb>,
    pub cursor: Option<Rgb>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// A part of the terminal, for [`Screen::text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextRange {
    /// The active screen.
    Screen,
    /// The primary screen's history, oldest line first.
    History,
}

/// The modes a program set that decide how input is encoded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputModes {
    /// DECCKM (mode 1): cursor keys send application sequences.
    pub application_cursor: bool,
    /// DECNKM (mode 66), or DECKPAM: the keypad sends application sequences.
    pub application_keypad: bool,
    /// Mode 2004: a paste is wrapped in `CSI 200 ~` and `CSI 201 ~`.
    pub bracketed_paste: bool,
    /// Mode 1004: the program wants `CSI I` and `CSI O` on focus changes.
    pub focus_reporting: bool,
    pub mouse_tracking: MouseTracking,
    pub mouse_format: MouseFormat,
    /// The kitty keyboard protocol's flags, 0 when it is off.
    pub kitty_keyboard: u8,
    /// Whether the alternate screen is shown.
    pub alternate_screen: bool,
}

/// Which mouse events the program asked for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MouseTracking {
    #[default]
    Off,
    /// Mode 9: presses only.
    X10,
    /// Mode 1000: presses and releases.
    Normal,
    /// Mode 1002: also motion while a button is held.
    Button,
    /// Mode 1003: all motion.
    Any,
}

/// How mouse events are encoded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MouseFormat {
    #[default]
    X10,
    /// Mode 1005
    Utf8,
    /// Mode 1006
    Sgr,
    /// Mode 1015
    Urxvt,
    /// Mode 1016
    SgrPixels,
}

/// The cursor on the active screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    /// The column, from zero.
    pub x: u16,
    /// The row, from zero.
    pub y: u16,
    pub visible: bool,
}

/// One cell of a row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cell {
    /// The grapheme cluster, or empty for a blank cell and for the second
    /// half of a wide character.
    pub text: String,
    /// 2 for a wide character, 0 for the cell its second half covers, and
    /// 1 otherwise.
    pub width: u8,
    pub style: Style,
    /// The OSC 8 hyperlink's URI.
    pub hyperlink: Option<String>,
}

/// The look of a cell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub foreground: Color,
    pub background: Color,
    pub underline_color: Color,
    pub underline: Underline,
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub blink: bool,
    pub inverse: bool,
    pub invisible: bool,
    pub strikethrough: bool,
    pub overline: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Color {
    /// The terminal's default colour.
    #[default]
    Default,
    /// An index into the 256-colour palette.
    Palette(u8),
    Rgb(Rgb),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Underline {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_byte_limit_grows_with_the_line_limit_and_the_line_limit_is_capped() {
        let at = |lines| Options {
            cols: 80,
            rows: 24,
            scrollback_lines: lines,
        };
        assert_eq!(at(10_000).scrollback_bytes(), 64 * 1024 * 1024);
        assert_eq!(at(100_000).scrollback_bytes(), 640 * 1024 * 1024);
        assert_eq!(
            at(10 * MAX_SCROLLBACK_LINES).scrollback_bytes(),
            at(MAX_SCROLLBACK_LINES).scrollback_bytes()
        );
    }
}
