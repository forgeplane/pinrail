//! A libghostty-vt terminal as the host creates one: grapheme clustering
//! on, unfinished sequences kept for a snapshot.

// only once build.rs built the library, which a stub build does not
#![cfg(ghostty_built)]

use pinrail_screen::Options;
use pinrail_screen::ghostty::Terminal;

fn terminal(cols: u16, rows: u16, scrollback_lines: usize) -> Terminal {
    Terminal::new(&Options {
        cols,
        rows,
        scrollback_lines,
    })
    .unwrap()
}

#[test]
fn grapheme_clustering_is_on() {
    let t = terminal(80, 24, 100);
    assert!(t.mode(2027, false));
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
