//! Random program output, painted for a new viewer: the viewer must show
//! the same cells, cursor, modes and title, and must go on to show the same
//! after more output, which also checks the pen, margins and wrap state
//! that the cells alone do not show.

use proptest::prelude::*;
use proptest::test_runner::{Config, TestCaseError, TestRunner};

use crate::New;
use crate::support::{difference, screen, viewer_of, write};

const COLS: u16 = 16;
const ROWS: u16 = 6;
const HISTORY: usize = 50;

/// Text in every width: ASCII, CJK, an emoji with VS16, a ZWJ sequence, and
/// a combining accent.
const TEXT: [&str; 7] = [
    "ab",
    "Hello, world",
    "日本",
    "\u{270d}\u{fe0f}",
    "\u{1f469}\u{200d}\u{1f4bb}",
    "e\u{301}",
    "|",
];

/// One escape sequence or run of text a program may write.
fn operation() -> impl Strategy<Value = String> {
    let n = || 1u8..5;
    let color = || 0u8..=255;
    prop_oneof![
        4 => proptest::sample::select(&TEXT[..]).prop_map(str::to_string),
        3 => proptest::sample::select(&[
            "1", "2", "3", "4", "4:3", "4:5", "5", "7", "8", "9", "21", "53", "22", "23",
            "24", "27", "29", "0", "31", "42", "97", "39", "49",
        ][..])
        .prop_map(|sgr| format!("\x1b[{sgr}m")),
        1 => color().prop_map(|c| format!("\x1b[38;5;{c}m")),
        1 => color().prop_map(|c| format!("\x1b[48;5;{c}m")),
        1 => (color(), color(), color()).prop_map(|(r, g, b)| format!("\x1b[38;2;{r};{g};{b}m")),
        1 => color().prop_map(|c| format!("\x1b[58;5;{c}m")),
        2 => (1..=ROWS + 1, 1..=COLS + 1).prop_map(|(r, c)| format!("\x1b[{r};{c}H")),
        2 => proptest::sample::select(&["\r", "\n", "\r\n", "\t", "\x08", "\x1bM", "\x1bE", "\x1bH"][..])
            .prop_map(str::to_string),
        2 => (proptest::sample::select(&["A", "B", "C", "D", "J", "K", "X", "@", "P", "L", "M", "S", "T", "g"][..]), 0u8..4)
            .prop_map(|(f, n)| format!("\x1b[{n}{f}")),
        1 => (1..=ROWS, 1..=ROWS).prop_map(|(t, b)| format!("\x1b[{t};{b}r")),
        2 => (proptest::sample::select(&[1u16, 6, 7, 9, 25, 66, 1000, 1002, 1003, 1004, 1005, 1006, 1015, 1016, 1049, 2004][..]), any::<bool>())
            .prop_map(|(m, on)| format!("\x1b[?{m}{}", if on { 'h' } else { 'l' })),
        1 => any::<bool>().prop_map(|on| format!("\x1b[4{}", if on { 'h' } else { 'l' })),
        1 => n().prop_map(|f| format!("\x1b[>{f}u")),
        1 => Just("\x1b[<u".to_string()),
        1 => proptest::sample::select(&["Build", "Tests", ""][..]).prop_map(|t| format!("\x1b]2;{t}\x07")),
        1 => Just("\x1b]8;;https://example.com\x1b\\".to_string()),
        1 => Just("\x1b]8;;\x1b\\".to_string()),
    ]
}

/// Output written to both screens after the paint.
const AFTER: &[u8] = "after 日\r\n\x1b[Kmore\x1b[3;1Hx".as_bytes();

fn check(new: New, output: &[u8]) -> Result<(), TestCaseError> {
    let mut s = screen(new, COLS, ROWS, HISTORY);
    write(&mut *s, output);
    let mut v = viewer_of(new, &*s, &s.first_paint(HISTORY), HISTORY);
    if let Some(difference) = difference(&*s, &*v) {
        return Err(TestCaseError::fail(format!(
            "after the paint: {difference}"
        )));
    }
    write(&mut *s, AFTER);
    write(&mut *v, AFTER);
    if let Some(difference) = difference(&*s, &*v) {
        return Err(TestCaseError::fail(format!(
            "after more output: {difference}"
        )));
    }
    Ok(())
}

pub fn random_output_survives_the_first_paint(new: New) {
    let mut runner = TestRunner::new(Config {
        cases: 512,
        failure_persistence: None,
        ..Config::default()
    });
    let output = proptest::collection::vec(operation(), 0..40).prop_map(|ops| ops.concat());
    if let Err(error) = runner.run(&output, |output| check(new, output.as_bytes())) {
        panic!("{error}");
    }
}
