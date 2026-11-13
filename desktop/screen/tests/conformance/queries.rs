//! The screen answers the queries a program sends, in order, so that a
//! program gets its answers when no viewer is attached.

use pinrail_screen::{ColorScheme, DisplayFacts, Rgb};

use crate::New;
use crate::support::{feed, screen};

fn reply(new: New, bytes: &[u8]) -> String {
    let mut s = screen(new, 80, 24, 100);
    String::from_utf8(feed(&mut *s, bytes).0).unwrap()
}

pub fn queries_are_answered_in_the_order_they_came(new: New) {
    // cursor position, then primary device attributes
    let answer = reply(new, b"\x1b[2;5H\x1b[6n\x1b[c");
    let attributes = answer
        .strip_prefix("\x1b[2;5R")
        .unwrap_or_else(|| panic!("{answer:?}"));
    assert!(
        attributes.starts_with("\x1b[?") && attributes.ends_with('c'),
        "{attributes:?}"
    );
}

pub fn the_terminal_names_itself_pinrail(new: New) {
    assert_eq!(
        reply(new, b"\x1b[>q"),
        format!("\x1bP>|Pinrail {}\x1b\\", env!("CARGO_PKG_VERSION"))
    );
}

pub fn size_reports_give_the_terminal_s_size(new: New) {
    let mut s = screen(new, 80, 24, 100);
    s.resize(100, 30, Some((9, 18)));
    // in cells, then in pixels
    let (replies, _) = feed(&mut *s, b"\x1b[18t\x1b[14t");
    assert_eq!(
        String::from_utf8(replies).unwrap(),
        "\x1b[8;30;100t\x1b[4;540;900t"
    );
}

pub fn the_colour_scheme_is_reported_once_known(new: New) {
    let mut s = screen(new, 80, 24, 100);
    assert_eq!(feed(&mut *s, b"\x1b[?996n").0, b"");
    let facts = |scheme| DisplayFacts {
        color_scheme: Some(scheme),
        ..DisplayFacts::default()
    };
    s.set_facts(&facts(ColorScheme::Dark));
    assert_eq!(feed(&mut *s, b"\x1b[?996n").0, b"\x1b[?997;1n");
    s.set_facts(&facts(ColorScheme::Light));
    assert_eq!(feed(&mut *s, b"\x1b[?996n").0, b"\x1b[?997;2n");
}

pub fn colour_queries_are_answered_from_the_display_facts(new: New) {
    let mut s = screen(new, 80, 24, 100);
    s.set_facts(&DisplayFacts {
        foreground: Some(Rgb {
            r: 0xee,
            g: 0xdd,
            b: 0xcc,
        }),
        background: Some(Rgb {
            r: 0x11,
            g: 0x22,
            b: 0x33,
        }),
        ..DisplayFacts::default()
    });
    let (replies, _) = feed(&mut *s, b"\x1b]10;?\x1b\\\x1b]11;?\x07");
    assert_eq!(
        String::from_utf8(replies).unwrap(),
        "\x1b]10;rgb:eeee/dddd/cccc\x1b\\\x1b]11;rgb:1111/2222/3333\x07"
    );
}

pub fn mode_and_keyboard_queries_are_answered(new: New) {
    let mut s = screen(new, 80, 24, 100);
    // bracketed paste on, then asked about; then the kitty keyboard flags
    let (replies, _) = feed(&mut *s, b"\x1b[?2004h\x1b[?2004$p\x1b[>1u\x1b[?u");
    assert_eq!(
        String::from_utf8(replies).unwrap(),
        "\x1b[?2004;1$y\x1b[?1u"
    );
}

pub fn the_program_status_probe_is_answered(new: New) {
    assert_eq!(
        reply(new, b"\x1b]7501;?\x1b\\"),
        "\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x1b\\"
    );
}
