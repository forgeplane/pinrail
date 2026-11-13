//! What the library answers to the queries a program sends, and the signals
//! it reports while parsing: replies go back to the program in order, and
//! signals to the host.

// only once build.rs built the library, which a stub build does not
#![cfg(ghostty_built)]

use pinrail_screen::ghostty::{
    ColorScheme, Options, ProgramState, ProgramStatus, ProgramWait, Signal, Terminal,
};

fn terminal() -> Terminal {
    Terminal::new(&Options {
        cols: 80,
        rows: 24,
        scrollback_lines: 100,
    })
    .unwrap()
}

/// What feeding `bytes` writes back to the program, and the signals.
fn feed(t: &mut Terminal, bytes: &[u8]) -> (Vec<u8>, Vec<Signal>) {
    let (mut replies, mut signals) = (Vec::new(), Vec::new());
    t.feed(bytes, &mut replies, &mut signals);
    (replies, signals)
}

fn reply(bytes: &[u8]) -> String {
    let mut t = terminal();
    String::from_utf8(feed(&mut t, bytes).0).unwrap()
}

#[test]
fn queries_are_answered_in_the_order_they_came() {
    // cursor position, then primary device attributes
    assert_eq!(reply(b"\x1b[6n\x1b[c"), "\x1b[1;1R\x1b[?62;22c");
}

#[test]
fn the_terminal_names_itself_pinrail() {
    let answer = reply(b"\x1b[>q");
    assert_eq!(
        answer,
        format!("\x1bP>|Pinrail {}\x1b\\", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn size_reports_give_the_terminal_s_size() {
    let mut t = terminal();
    t.resize(100, 30, Some((9, 18))).unwrap();
    // in cells, then in pixels
    let (replies, _) = feed(&mut t, b"\x1b[18t\x1b[14t");
    assert_eq!(
        String::from_utf8(replies).unwrap(),
        "\x1b[8;30;100t\x1b[4;540;900t"
    );
}

#[test]
fn the_colour_scheme_is_reported_once_known() {
    let mut t = terminal();
    assert_eq!(feed(&mut t, b"\x1b[?996n").0, b"");
    t.set_color_scheme(Some(ColorScheme::Dark));
    assert_eq!(feed(&mut t, b"\x1b[?996n").0, b"\x1b[?997;1n");
    t.set_color_scheme(Some(ColorScheme::Light));
    assert_eq!(feed(&mut t, b"\x1b[?996n").0, b"\x1b[?997;2n");
}

#[test]
fn the_program_status_probe_is_answered() {
    assert_eq!(
        reply(b"\x1b]7501;?\x1b\\"),
        "\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x1b\\"
    );
}

#[test]
fn a_program_status_report_is_a_signal() {
    let mut t = terminal();
    let (_, signals) = feed(
        &mut t,
        b"\x1b]7501;state=blocked:kind=permission:app=claude-code:msg=QXBwbHk/\x1b\\",
    );
    assert_eq!(
        signals,
        [Signal::ProgramStatus(ProgramStatus {
            state: ProgramState::Blocked,
            wait: Some(ProgramWait::Permission),
            progress: None,
            id: String::new(),
            app: "claude-code".to_string(),
            title: String::new(),
            message: "Apply?".to_string(),
        })]
    );
}

#[test]
fn the_title_bell_and_notifications_are_signals() {
    let mut t = terminal();
    let (_, signals) = feed(
        &mut t,
        b"\x1b]0;Fixing the build\x07\x07\x1b]9;Tests passed\x07\x1b]777;notify;Codex;Needs you\x07",
    );
    assert_eq!(
        signals,
        [
            Signal::Title("Fixing the build".to_string()),
            Signal::Bell,
            Signal::Notify {
                title: None,
                body: "Tests passed".to_string()
            },
            Signal::Notify {
                title: Some("Codex".to_string()),
                body: "Needs you".to_string()
            },
        ]
    );
}

#[test]
fn turning_focus_reporting_on_or_off_is_a_signal() {
    let mut t = terminal();
    assert_eq!(
        feed(&mut t, b"\x1b[?1004h").1,
        [Signal::FocusReporting(true)]
    );
    // setting it again changes nothing
    assert_eq!(feed(&mut t, b"\x1b[?1004h").1, []);
    assert_eq!(
        feed(&mut t, b"\x1b[?1004l").1,
        [Signal::FocusReporting(false)]
    );
}
