//! A viewer that attaches gets the terminal as it is, from the first paint
//! any VT emulator can read: history, the alternate screen, and an
//! unfinished escape sequence.

use pinrail_screen::{Color, MouseTracking, Screen, TextRange, Underline};

use crate::New;
use crate::support::{assert_same, screen, viewer_of, write};

/// A screen a program has used: numbered history, a styled line, a title,
/// and the cursor left mid-line.
fn used(new: New) -> Box<dyn Screen> {
    let mut s = screen(new, 60, 12, 10_000);
    for i in 0..300 {
        write(&mut *s, format!("history line {i}\r\n").as_bytes());
    }
    write(
        &mut *s,
        b"\x1b]2;Agent at work\x07\x1b[1;31mred bold\x1b[0m plain\r\nprompt> ab",
    );
    s
}

pub fn the_first_paint_rebuilds_the_screen_cursor_and_title(new: New) {
    let s = used(new);
    let v = viewer_of(new, &*s, &s.first_paint(0), 10_000);
    assert_same(&*s, &*v);
    assert_eq!(v.title().as_deref(), Some("Agent at work"));
}

pub fn the_first_paint_starts_by_cancelling_and_resetting(new: New) {
    let paint = used(new).first_paint(0);
    assert!(paint.starts_with(b"\x18\x1b[!p"), "{:?}", &paint[..8]);
}

pub fn the_first_paint_pushes_the_history_asked_for_into_the_scrollback(new: New) {
    let s = used(new);
    let v = viewer_of(new, &*s, &s.first_paint(100), 10_000);
    assert!(v.retained_lines() >= 100, "kept {}", v.retained_lines());
    // the 100 lines above the screen, which shows lines 290 and on
    let history = v.text(TextRange::History);
    assert!(history.contains("history line 190"), "{history}");
    assert!(history.contains("history line 289"));
    assert!(!history.contains("history line 18"));
    assert_same(&*s, &*v);
}

pub fn the_first_paint_shows_the_alternate_screen_over_the_primary_one(new: New) {
    let mut s = used(new);
    let primary = s.text(TextRange::Screen);
    write(&mut *s, b"\x1b[?1049h\x1b[H\x1b[2Jfull-screen editor");
    let mut v = viewer_of(new, &*s, &s.first_paint(50), 10_000);
    assert!(v.modes().alternate_screen);
    assert_same(&*s, &*v);
    // the primary screen behind it is the program's, not blank
    write(&mut *v, b"\x1b[?1049l");
    assert_eq!(v.text(TextRange::Screen), primary);
}

pub fn synchronized_output_left_on_does_not_freeze_the_viewer(new: New) {
    let mut s = used(new);
    write(&mut *s, b"\x1b[?2026h");
    let paint = s.first_paint(0);
    assert!(!paint.windows(8).any(|w| w == b"\x1b[?2026h"));
    // the viewer's own paint has it off as well
    let v = viewer_of(new, &*s, &paint, 10_000);
    assert!(!v.first_paint(0).windows(8).any(|w| w == b"\x1b[?2026h"));
}

pub fn a_paint_taken_mid_sequence_ends_with_it(new: New) {
    let mut s = used(new);
    write(&mut *s, b"\x1b]2;New ti");
    let mut v = viewer_of(new, &*s, &s.first_paint(0), 10_000);
    write(&mut *v, b"tle\x07");
    assert_eq!(v.title().as_deref(), Some("New title"));
}

pub fn styles_hyperlinks_and_input_modes_survive_the_first_paint(new: New) {
    let mut s = screen(new, 40, 6, 100);
    write(
        &mut *s,
        b"\x1b[3;4:3;58;5;196m\x1b[38;2;1;2;3;48;5;17mstyled\x1b[0m \
          \x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\ \x1b[44m\x1b[K\x1b[0m\
          \x1b[?1h\x1b[?2004h\x1b[?1002h\x1b[?1006h\x1b[>5u\x1b[?25l",
    );
    let v = viewer_of(new, &*s, &s.first_paint(0), 100);
    assert_same(&*s, &*v);
    let row = v.row(0);
    let style = row[0].style;
    assert!(style.italic);
    assert_eq!(style.underline, Underline::Curly);
    assert_eq!(style.underline_color, Color::Palette(196));
    assert_eq!(style.background, Color::Palette(17));
    assert_eq!(row[7].hyperlink.as_deref(), Some("https://example.com"));
    // the erased rest of the line keeps its background
    assert_eq!(row[39].style.background, Color::Palette(4));
    let modes = v.modes();
    assert!(modes.application_cursor && modes.bracketed_paste);
    assert_eq!(modes.mouse_tracking, MouseTracking::Button);
    assert_eq!(modes.kitty_keyboard, 5);
    assert!(!v.cursor().visible);
}
