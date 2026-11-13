//! A terminal as the host creates one: its history limits, grapheme
//! clustering that survives a reset, unfinished sequences kept for a
//! snapshot, writing and resizing.

// only once build.rs built the library, which a stub build does not
#![cfg(ghostty_built)]

use pinrail_screen::ghostty::{MAX_SCROLLBACK_LINES, Options, Terminal};

fn terminal(cols: u16, rows: u16, scrollback_lines: usize) -> Terminal {
    Terminal::new(&Options {
        cols,
        rows,
        scrollback_lines,
    })
    .unwrap()
}

/// `count` numbered lines of program output.
fn lines(count: usize) -> Vec<u8> {
    (0..count)
        .flat_map(|i| format!("line {i}\r\n").into_bytes())
        .collect()
}

#[test]
fn history_is_kept_up_to_about_the_line_limit() {
    let mut t = terminal(80, 24, 1_000);
    t.write(&lines(5_000));
    let kept = t.retained_lines();
    // pruned by whole pages, so near the limit rather than at it
    assert!((800..=1_500).contains(&kept), "kept {kept} lines");
}

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

#[test]
fn grapheme_clustering_is_on_and_survives_a_reset() {
    let mut t = terminal(80, 24, 100);
    assert!(t.mode(2027, false));
    // RIS
    t.write(b"\x1bc");
    assert!(t.mode(2027, false));
    // an emoji with VS16 takes two cells, as the agents' renderers draw it
    t.write("\u{270d}\u{fe0f}".as_bytes());
    assert_eq!(t.cursor(), (2, 0));
}

#[test]
fn an_unfinished_escape_sequence_is_kept_for_a_snapshot() {
    let mut t = terminal(80, 24, 100);
    t.write(b"ready\x1b]0;half a ti");
    assert_eq!(t.continuation().as_deref(), Some(&b"\x1b]0;half a ti"[..]));
    t.write(b"tle\x07");
    assert_eq!(t.continuation().as_deref(), Some(&b""[..]));
}

#[test]
fn writing_moves_the_cursor_and_resizing_changes_the_size() {
    let mut t = terminal(80, 24, 100);
    t.write(b"abc\r\nde");
    assert_eq!(t.cursor(), (2, 1));
    t.resize(120, 40, Some((9, 18))).unwrap();
    assert_eq!(t.size(), (120, 40));
    // the cell size stays when none is given
    t.resize(100, 30, None).unwrap();
    assert_eq!(t.size(), (100, 30));
    assert!(t.resize(0, 30, None).is_err());
}

#[test]
fn a_terminal_of_no_size_is_refused() {
    assert!(
        Terminal::new(&Options {
            cols: 0,
            rows: 24,
            scrollback_lines: 100
        })
        .is_err()
    );
}
