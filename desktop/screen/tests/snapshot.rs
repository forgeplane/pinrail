//! libghostty-vt's own binary snapshot restores a terminal exactly,
//! including history, modes and an unfinished escape sequence.

// only once build.rs built the library, which a stub build does not
#![cfg(ghostty_built)]

use pinrail_screen::ghostty::Terminal;
use pinrail_screen::{Options, Screen, TextRange};

/// A terminal a program has used: numbered history, a styled line, a
/// title, and the cursor left mid-line.
fn used() -> Terminal {
    let mut t = Terminal::new(&Options {
        cols: 60,
        rows: 12,
        scrollback_lines: 10_000,
    })
    .unwrap();
    for i in 0..300 {
        t.write(format!("history line {i}\r\n").as_bytes());
    }
    t.write(b"\x1b]2;Agent at work\x07\x1b[1;31mred bold\x1b[0m plain\r\nprompt> ab");
    t
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
    assert_eq!(r.text(TextRange::Screen), t.text(TextRange::Screen));
    assert_eq!(r.text(TextRange::History), t.text(TextRange::History));
    assert_eq!(r.retained_lines(), t.retained_lines());
    assert_eq!(r.cursor(), t.cursor());
    assert!(r.mode(2027, false));
    // the half-received title completes in the restored terminal
    r.write(b"itten\x07");
    assert_eq!(r.title().as_deref(), Some("Half written"));
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

#[test]
fn a_snapshot_is_possible_only_outside_a_sequence_longer_than_the_continuation() {
    let mut t = used();
    assert!(t.paintable());
    // an unfinished title fits in the continuation
    t.write(b"\x1b]2;Half wr");
    assert!(t.paintable());
    t.write(b"itten\x07");

    // an image longer than the continuation does not
    t.write(b"\x1b_Gf=100;");
    t.write(&vec![b'A'; 100_000]);
    assert!(!t.paintable());
    t.write(&vec![b'A'; 1_000]);
    assert!(!t.paintable());
    t.write(b"\x1b\\");
    assert!(t.paintable());

    // a new sequence that starts after the long one ends is replayable
    t.write(b"\x1b_Gf=100;");
    t.write(&vec![b'A'; 100_000]);
    t.write(b"\x1b\\\x1b]2;Next");
    assert!(t.paintable());
}
