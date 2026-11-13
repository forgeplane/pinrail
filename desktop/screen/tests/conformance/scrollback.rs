//! History is kept up to the line limit, and up to the byte limit for a
//! terminal so wide that the line limit would take too much memory.

use crate::New;
use crate::support::{screen, write};

pub fn history_is_kept_up_to_about_the_line_limit(new: New) {
    let mut s = screen(new, 80, 24, 1_000);
    let lines: Vec<u8> = (0..5_000)
        .flat_map(|i| format!("line {i}\r\n").into_bytes())
        .collect();
    write(&mut *s, &lines);
    let kept = s.retained_lines();
    // an implementation may prune by block, so near the limit
    assert!((800..=1_500).contains(&kept), "kept {kept} lines");
}

pub fn a_wide_terminal_stops_at_the_byte_limit(new: New) {
    // 10,000 lines of 2,000 columns take far more than the 64 MiB that the
    // byte limit allows for 10,000 lines
    let mut s = screen(new, 2_000, 10, 10_000);
    let mut line = vec![b'x'; 2_000];
    line.extend(b"\r\n");
    for _ in 0..12_000 {
        write(&mut *s, &line);
    }
    let kept = s.retained_lines();
    assert!((1..8_000).contains(&kept), "kept {kept} lines");
}

pub fn resizing_changes_the_size(new: New) {
    let mut s = screen(new, 80, 24, 100);
    s.resize(120, 40, Some((9, 18)));
    assert_eq!(s.size(), (120, 40));
    // the cell size stays when none is given, and no size is ignored
    s.resize(100, 30, None);
    s.resize(0, 30, None);
    assert_eq!(s.size(), (100, 30));
}
