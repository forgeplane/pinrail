//! What the tests share: creating a screen, writing to it, and comparing
//! two screens cell by cell.

use pinrail_screen::{Cell, Color, Options, Screen, Signal, Style, TextRange};

use crate::New;

pub fn screen(new: New, cols: u16, rows: u16, scrollback_lines: usize) -> Box<dyn Screen> {
    new(&Options {
        cols,
        rows,
        scrollback_lines,
    })
}

/// What feeding `bytes` writes back to the program, and the signals.
pub fn feed(s: &mut dyn Screen, bytes: &[u8]) -> (Vec<u8>, Vec<Signal>) {
    let (mut replies, mut signals) = (Vec::new(), Vec::new());
    s.feed(bytes, &mut replies, &mut signals);
    (replies, signals)
}

pub fn write(s: &mut dyn Screen, bytes: &[u8]) {
    feed(s, bytes);
}

/// A fresh screen of the same size and history limit, after reading
/// `bytes`.
pub fn viewer_of(
    new: New,
    s: &dyn Screen,
    bytes: &[u8],
    scrollback_lines: usize,
) -> Box<dyn Screen> {
    let (cols, rows) = s.size();
    let mut v = screen(new, cols, rows, scrollback_lines);
    write(&mut *v, bytes);
    v
}

/// Where two screens differ, or `None` when they show the same: the size,
/// the cells of the active screen, the cursor, the input modes and the
/// title.
pub fn difference(a: &dyn Screen, b: &dyn Screen) -> Option<String> {
    if a.size() != b.size() {
        return Some(format!("size {:?} != {:?}", a.size(), b.size()));
    }
    let (_, rows) = a.size();
    for y in 0..rows {
        let (left, right) = (shown(a, y), shown(b, y));
        if left != right {
            let x = left
                .iter()
                .zip(&right)
                .position(|(l, r)| l != r)
                .unwrap_or(left.len().min(right.len()));
            return Some(format!(
                "row {y} differs at column {x}: {:?} != {:?}\n{}\n---\n{}",
                left.get(x),
                right.get(x),
                a.text(TextRange::Screen),
                b.text(TextRange::Screen),
            ));
        }
    }
    if a.cursor() != b.cursor() {
        return Some(format!("cursor {:?} != {:?}", a.cursor(), b.cursor()));
    }
    if a.modes() != b.modes() {
        return Some(format!("modes {:?} != {:?}", a.modes(), b.modes()));
    }
    if a.title() != b.title() {
        return Some(format!("title {:?} != {:?}", a.title(), b.title()));
    }
    None
}

#[track_caller]
pub fn assert_same(a: &dyn Screen, b: &dyn Screen) {
    if let Some(difference) = difference(a, b) {
        panic!("{difference}");
    }
}

/// The cells of a row as they look. A cell without text, or with a space,
/// shows only its background, and its foreground when inverse, so its
/// other attributes are dropped.
///
/// A wide character that does not fit at the end of a line leaves a blank
/// cell there and goes on the next line. libghostty-vt's formatter does not
/// keep that cell's colours, so the style of a blank cell in the last
/// column is not compared. A line erased with a background still shows in
/// the other columns.
fn shown(s: &dyn Screen, y: u16) -> Vec<Cell> {
    let mut row: Vec<Cell> = s.row(y).into_iter().map(blank_as_shown).collect();
    if let Some(last) = row.last_mut()
        && last.text.is_empty()
    {
        last.style = Style::default();
    }
    row
}

fn blank_as_shown(cell: Cell) -> Cell {
    if !(cell.text.is_empty() || cell.text == " ") {
        return cell;
    }
    let style = cell.style;
    Cell {
        text: String::new(),
        style: Style {
            background: style.background,
            inverse: style.inverse,
            foreground: if style.inverse {
                style.foreground
            } else {
                Color::Default
            },
            ..Style::default()
        },
        hyperlink: None,
        ..cell
    }
}
