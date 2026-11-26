//! Everything built into libghostty-vt has its licence in the notices the
//! app ships.

// only once build.rs built the library, which a stub build does not
#![cfg(ghostty_built)]

use std::process::Command;

/// The notices file, from the repository's root.
const NOTICES: &str = include_str!("../../../licenses/libghostty-vt.txt");

/// The heading of the notice each object file of the archive falls under,
/// by the start of its name.
const OBJECTS: [(&str, &str); 12] = [
    ("ghostty-vt-static_zcu", "Ghostty"),
    ("vt", "Ghostty"),
    ("index_of", "Ghostty"),
    ("codepoint_width", "Ghostty"),
    ("base64", "Ghostty"),
    ("abort", "Ghostty"),
    ("simdutf", "simdutf"),
    ("highway_zcu", "Highway"),
    ("targets", "Highway"),
    ("per_target", "Highway"),
    ("wuffs", "Wuffs"),
    ("compiler_rt", "Zig's compiler_rt"),
];

/// What the Zig code compiles into Ghostty's own objects, which no object
/// name shows.
const COMPILED_IN: [&str; 3] = ["uucode", "Bjoern Hoehrmann", "Unicode License v3"];

/// The headings of the notices file: the line after each rule.
fn headings() -> Vec<&'static str> {
    let lines: Vec<&str> = NOTICES.lines().collect();
    lines
        .windows(3)
        .filter(|w| w[0].starts_with("-----"))
        .map(|w| w[2])
        .collect()
}

#[test]
fn every_part_of_the_library_has_its_licence_in_the_notices() {
    let archive = env!("PINRAIL_GHOSTTY_ARCHIVE");
    let listed = Command::new("ar").arg("t").arg(archive).output().unwrap();
    assert!(listed.status.success(), "ar t {archive}");
    let headings = headings();
    for object in String::from_utf8(listed.stdout).unwrap().lines() {
        if !object.ends_with(".o") {
            continue;
        }
        // GNU ar lists a member with the path Zig built it at, such as
        // .zig-cache/o/<hash>/base64.o
        let name = object.rsplit('/').next().unwrap().trim_end_matches(".o");
        let Some((_, heading)) = OBJECTS
            .iter()
            .find(|(start, _)| name == *start || name.starts_with(&format!("{start}-")))
        else {
            panic!(
                "{object} is in libghostty-vt, but no notice covers it: add its licence to \
                 licenses/libghostty-vt.txt and its name to OBJECTS"
            );
        };
        assert!(
            headings.iter().any(|h| h.starts_with(heading)),
            "{object} falls under {heading}, which licenses/libghostty-vt.txt lacks"
        );
    }
    for part in COMPILED_IN {
        assert!(
            headings.iter().any(|h| h.contains(part)),
            "licenses/libghostty-vt.txt lacks {part}"
        );
    }
}

#[test]
fn the_app_s_notices_include_libghostty_vt() {
    let script = include_str!("../../app/scripts/build-notices.mjs");
    assert!(
        script.contains("licenses\", \"libghostty-vt.txt\""),
        "build-notices.mjs does not add licenses/libghostty-vt.txt to the app's notices"
    );
}
