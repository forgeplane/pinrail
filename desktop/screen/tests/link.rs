//! The library build.rs builds is the one ghostty.lock pins, and links.

// only once build.rs built the library, which a stub build does not
#[cfg(ghostty_built)]
#[test]
fn the_linked_library_is_the_pinned_release() {
    let lock = include_str!("../ghostty.lock");
    let pinned = lock
        .lines()
        .find_map(|line| line.strip_prefix("version = "))
        .map(|v| v.trim_matches('"'))
        .unwrap();
    assert_eq!(pinrail_screen::ghostty::version(), pinned);
}
