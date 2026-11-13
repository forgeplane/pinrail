//! A viewer that attaches gets the terminal as it is: the first paint any
//! VT emulator can read, or libghostty-vt's own binary snapshot, including
//! history, the alternate screen and an unfinished escape sequence.

// only once build.rs built the library, which a stub build does not
#![cfg(ghostty_built)]

use pinrail_screen::ghostty::{Options, Terminal};

fn terminal(cols: u16, rows: u16) -> Terminal {
    Terminal::new(&Options {
        cols,
        rows,
        scrollback_lines: 10_000,
    })
    .unwrap()
}

/// A terminal a program has used: numbered history, a styled line, a
/// title, and the cursor left mid-line.
fn used() -> Terminal {
    let mut t = terminal(60, 12);
    for i in 0..300 {
        t.write(format!("history line {i}\r\n").as_bytes());
    }
    t.write(b"\x1b]2;Agent at work\x07\x1b[1;31mred bold\x1b[0m plain\r\nprompt> ab");
    t
}

/// A fresh viewer of the same size, after reading `paint`.
fn viewer_of(paint: &[u8], cols: u16, rows: u16) -> Terminal {
    let mut v = terminal(cols, rows);
    v.write(paint);
    v
}

#[test]
fn the_first_paint_rebuilds_the_screen_cursor_and_title_in_a_fresh_terminal() {
    let t = used();
    let v = viewer_of(&t.first_paint(0), 60, 12);
    assert_eq!(v.screen_text(), t.screen_text());
    assert_eq!(v.cursor(), t.cursor());
    assert_eq!(v.title(), "Agent at work");
}

#[test]
fn the_first_paint_starts_by_cancelling_and_resetting() {
    let paint = used().first_paint(0);
    assert!(paint.starts_with(b"\x18\x1b[!p"), "{:?}", &paint[..8]);
}

#[test]
fn the_first_paint_pushes_the_history_asked_for_into_the_viewer_s_scrollback() {
    let t = used();
    let v = viewer_of(&t.first_paint(100), 60, 12);
    assert!(v.retained_lines() >= 100, "kept {}", v.retained_lines());
    // the 100 lines above the screen, which shows lines 290 and on
    let history = v.history_text();
    assert!(history.contains("history line 190"), "{history}");
    assert!(history.contains("history line 289"));
    assert!(!history.contains("history line 18"));
    assert_eq!(v.screen_text(), t.screen_text());
}

#[test]
fn the_first_paint_shows_the_alternate_screen_over_the_primary_one() {
    let mut t = used();
    let primary = t.screen_text();
    t.write(b"\x1b[?1049h\x1b[H\x1b[2Jfull-screen editor");
    let v = viewer_of(&t.first_paint(50), 60, 12);
    assert!(v.alternate_screen());
    assert_eq!(v.screen_text(), t.screen_text());
    // the primary screen behind it is the program's, not blank
    let mut back = v;
    back.write(b"\x1b[?1049l");
    assert_eq!(back.screen_text(), primary);
}

#[test]
fn synchronized_output_left_on_does_not_freeze_the_viewer() {
    let mut t = used();
    t.write(b"\x1b[?2026h");
    let paint = t.first_paint(0);
    assert!(!paint.windows(8).any(|w| w == b"\x1b[?2026h"));
    assert!(!viewer_of(&paint, 60, 12).mode(2026, false));
}

#[test]
fn a_paint_taken_mid_sequence_ends_with_it_so_the_rest_completes_it() {
    let mut t = used();
    t.write(b"\x1b]2;New ti");
    let mut v = viewer_of(&t.first_paint(0), 60, 12);
    v.write(b"tle\x07");
    assert_eq!(v.title(), "New title");
}

#[test]
fn a_binary_snapshot_restores_the_terminal_exactly() {
    let mut t = used();
    t.write(b"\x1b]2;Half wr");
    let snapshot = t.binary_snapshot().unwrap();
    assert_eq!(
        snapshot.format,
        format!("ghostty-bin;vt-{}", pinrail_screen::ghostty::version())
    );
    let mut r = Terminal::restore(&snapshot.bytes, 10_000).unwrap();
    assert_eq!(r.screen_text(), t.screen_text());
    assert_eq!(r.history_text(), t.history_text());
    assert_eq!(r.retained_lines(), t.retained_lines());
    assert_eq!(r.cursor(), t.cursor());
    assert!(r.mode(2027, false));
    // the half-received title completes in the restored terminal
    r.write(b"itten\x07");
    assert_eq!(r.title(), "Half written");
}

#[test]
fn a_restored_terminal_answers_queries_too() {
    let snapshot = used().binary_snapshot().unwrap();
    let mut r = Terminal::restore(&snapshot.bytes, 10_000).unwrap();
    let (mut replies, mut signals) = (Vec::new(), Vec::new());
    r.feed(b"\x1b[c", &mut replies, &mut signals);
    assert_eq!(replies, b"\x1b[?62;22c");
}

#[test]
fn a_snapshot_that_does_not_decode_is_refused() {
    assert!(Terminal::restore(b"not a snapshot", 10_000).is_err());
}
