//! The query filter removes from the viewers' stream every query the
//! host's screen answered, however the output is split into chunks, and
//! passes everything else unchanged.

use pinrail_host::filter::QueryFilter;
use pinrail_screen::{ColorScheme, DisplayFacts, Options, Rgb, Screen, Signal};

/// What the viewers see and what the screen answered, for `chunks` of
/// output, and the screen's text.
fn run(chunks: &[&[u8]]) -> (Vec<u8>, Vec<u8>, String) {
    let mut screen = pinrail_screen::ghostty::Terminal::new(&Options {
        cols: 80,
        rows: 24,
        scrollback_lines: 100,
    })
    .unwrap();
    // the viewer's colours, which the colour queries are answered from
    let rgb = |v| Some(Rgb { r: v, g: v, b: v });
    screen.set_facts(&DisplayFacts {
        color_scheme: Some(ColorScheme::Dark),
        foreground: rgb(0xee),
        background: rgb(0x11),
        cursor: None,
    });
    let mut filter = QueryFilter::default();
    let (mut viewers, mut replies) = (Vec::new(), Vec::new());
    let mut feed = |bytes: &[u8]| {
        let before = replies.len();
        let mut signals: Vec<Signal> = Vec::new();
        screen.feed(bytes, &mut replies, &mut signals);
        replies.len() - before
    };
    for chunk in chunks {
        filter.push(chunk, &mut viewers, &mut feed);
    }
    let text = screen.text(pinrail_screen::TextRange::Screen);
    (viewers, replies, text)
}

/// Queries the screen answers.
const QUERIES: &[&[u8]] = &[
    b"\x1b[c",
    b"\x1b[6n",
    b"\x1b[?2004$p",
    b"\x1b[>q",
    b"\x1b[18t",
    b"\x1b[?u",
    b"\x1b]10;?\x07",
    b"\x1b]11;?\x1b\\",
    b"\x1b]7501;?\x1b\\",
    b"\x1b[?996n",
];

#[test]
fn an_answered_query_split_at_any_byte_is_removed_for_viewers() {
    for query in QUERIES {
        let whole: Vec<u8> = [&b"before\x1b[1mbold"[..], query, b"after"].concat();
        let (viewers, replies, text) = run(&[&whole]);
        assert_eq!(
            viewers,
            b"before\x1b[1mboldafter",
            "{:?}",
            String::from_utf8_lossy(query)
        );
        assert!(!replies.is_empty(), "{:?}", String::from_utf8_lossy(query));
        for split in 1..whole.len() {
            let (first, second) = whole.split_at(split);
            let (split_viewers, split_replies, split_text) = run(&[first, second]);
            assert_eq!(
                split_viewers,
                viewers,
                "{:?} split at {split}",
                String::from_utf8_lossy(query)
            );
            assert_eq!(split_replies, replies);
            assert_eq!(split_text, text);
        }
    }
}

#[test]
fn output_fed_one_byte_at_a_time_reads_as_in_one_chunk() {
    let whole: Vec<u8> = [
        &b"\x1b]0;title\x07line one\r\n\x1b[31mred\x1b[0m"[..],
        b"\x1b[c\x1b]7501;?\x07\x1b[6n",
        "日本 ✍\u{fe0f}\r\n\x1b[2J\x1b[H".as_bytes(),
        b"\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\",
    ]
    .concat();
    let (viewers, replies, text) = run(&[&whole]);
    let bytes: Vec<&[u8]> = whole.chunks(1).collect();
    let (one, one_replies, one_text) = run(&bytes);
    assert_eq!(one, viewers);
    assert_eq!(one_replies, replies);
    assert_eq!(one_text, text);
    // only the three queries were taken out
    assert_eq!(
        viewers.len(),
        whole.len() - b"\x1b[c\x1b]7501;?\x07\x1b[6n".len()
    );
}

#[test]
fn a_query_the_screen_does_not_answer_passes_to_the_viewers() {
    // a clipboard read, which the host never answers
    let output = b"x\x1b]52;c;?\x07y";
    assert_eq!(run(&[output]).0, output);
}

#[test]
fn sequences_over_the_caps_pass_unchanged() {
    let long_csi: Vec<u8> = [&b"\x1b["[..], &b"1;".repeat(40), b"c"].concat();
    assert_eq!(run(&[&long_csi]).0, long_csi);
    let long_osc: Vec<u8> = [&b"\x1b]10;?"[..], &b"x".repeat(600), b"\x07"].concat();
    let halves: Vec<&[u8]> = long_osc.chunks(100).collect();
    assert_eq!(run(&halves).0, long_osc);
}

#[test]
fn cancelled_and_interrupted_sequences_pass_unchanged() {
    let output = b"a\x1b[1\x18b\x1b]0;half\x1b[cc";
    let (viewers, replies, _) = run(&[output]);
    // the cancelled CSI and the OSC cut by a new sequence pass; the
    // device attributes query after them is answered and removed
    assert_eq!(viewers, b"a\x1b[1\x18b\x1b]0;halfc");
    assert!(!replies.is_empty());
}

#[test]
fn output_without_a_query_reaches_the_screen_in_one_call_per_chunk() {
    // each call to the screen has a cost of its own, so colour sequences
    // and text go together
    let mut filter = QueryFilter::default();
    let (mut viewers, mut calls) = (Vec::new(), Vec::new());
    let chunk: &[u8] = b"\x1b[1;31mred\x1b[0m \x1b]2;title\x07\x1b[?2026hframe\x1b[?2026l\r\n";
    filter.push(chunk, &mut viewers, &mut |bytes: &[u8]| {
        calls.push(bytes.to_vec());
        0
    });
    assert_eq!(viewers, chunk);
    assert_eq!(calls, [chunk.to_vec()]);
}

#[test]
fn a_query_among_colours_is_still_answered_and_removed() {
    let (viewers, replies, text) = run(&[b"\x1b[32mgreen\x1b[0m\x1b[6n\x1b[1mbold\x1b[0m"]);
    assert_eq!(viewers, b"\x1b[32mgreen\x1b[0m\x1b[1mbold\x1b[0m");
    assert!(replies.starts_with(b"\x1b[1;"), "{replies:?}");
    assert!(text.contains("greenbold"), "{text}");
}
