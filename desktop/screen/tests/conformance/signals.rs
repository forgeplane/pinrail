//! What a program tells its terminal, other than screen content, is
//! reported as signals.

use pinrail_screen::{ProgramState, ProgramStatus, ProgramWait, Signal};

use crate::New;
use crate::support::{feed, screen};

pub fn a_program_status_report_is_a_signal(new: New) {
    let mut s = screen(new, 80, 24, 100);
    let (_, signals) = feed(
        &mut *s,
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

pub fn the_title_bell_and_notifications_are_signals(new: New) {
    let mut s = screen(new, 80, 24, 100);
    let (_, signals) = feed(
        &mut *s,
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

pub fn turning_focus_reporting_on_or_off_is_a_signal(new: New) {
    let mut s = screen(new, 80, 24, 100);
    assert_eq!(
        feed(&mut *s, b"\x1b[?1004h").1,
        [Signal::FocusReporting(true)]
    );
    // setting it again changes nothing
    assert_eq!(feed(&mut *s, b"\x1b[?1004h").1, []);
    assert_eq!(
        feed(&mut *s, b"\x1b[?1004l").1,
        [Signal::FocusReporting(false)]
    );
}
