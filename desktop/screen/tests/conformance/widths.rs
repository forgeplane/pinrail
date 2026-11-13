//! Characters take the widths the agents' renderers give them, so that the
//! cursor never lands inside a wide character after a first paint.

use pinrail_screen::Cursor;

use crate::New;
use crate::support::{assert_same, screen, viewer_of, write};

/// Writes `text` to a fresh screen, checks that the cursor moved `width`
/// cells and that the first cell holds the whole cluster, and that a first
/// paint rebuilds it.
fn takes(new: New, text: &str, width: u16) {
    let mut s = screen(new, 20, 4, 100);
    write(&mut *s, text.as_bytes());
    assert_eq!(s.cursor().x, width, "{text:?}");
    let row = s.row(0);
    assert_eq!(row[0].text, text);
    if width == 2 {
        assert_eq!((row[0].width, row[1].width), (2, 0));
    }
    let v = viewer_of(new, &*s, &s.first_paint(0), 100);
    assert_same(&*s, &*v);
}

pub fn emoji_with_vs16_take_two_cells_even_after_a_reset(new: New) {
    takes(new, "\u{270d}\u{fe0f}", 2);
    let mut s = screen(new, 20, 4, 100);
    // RIS
    write(&mut *s, "\x1bc\u{270d}\u{fe0f}".as_bytes());
    assert_eq!(
        s.cursor(),
        Cursor {
            x: 2,
            y: 0,
            visible: true
        }
    );
}

pub fn zwj_sequences_take_two_cells(new: New) {
    // a family, and a woman technologist
    takes(new, "\u{1f469}\u{200d}\u{1f469}\u{200d}\u{1f467}", 2);
    takes(new, "\u{1f469}\u{200d}\u{1f4bb}", 2);
}

pub fn cjk_takes_two_cells(new: New) {
    takes(new, "日", 2);
    let mut s = screen(new, 20, 4, 100);
    write(&mut *s, "日本語です".as_bytes());
    assert_eq!(s.cursor().x, 10);
}
