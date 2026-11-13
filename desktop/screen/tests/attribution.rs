//! Code adapted from herdr-core carries its licence into the app's notices.

use std::path::Path;

/// The files under `dir` whose text says they are adapted from herdr-core.
fn adapted(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(adapted(&path));
        } else if path.extension().is_some_and(|e| e == "rs")
            && std::fs::read_to_string(&path)
                .unwrap()
                .contains("Adapted from herdr-core (Apache-2.0)")
        {
            found.push(path.display().to_string());
        }
    }
    found
}

#[test]
fn code_adapted_from_herdr_core_has_its_licence_in_the_notices() {
    let desktop = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let files = adapted(&desktop.join("screen").join("src"));
    assert!(!files.is_empty(), "no adapted file found to check");
    let script = include_str!("../../app/scripts/build-notices.mjs");
    assert!(
        script.contains("licenses\", \"herdr-core.txt\""),
        "{files:?} adapt herdr-core, but build-notices.mjs does not add licenses/herdr-core.txt"
    );
    let licence = include_str!("../../../licenses/herdr-core.txt");
    assert!(licence.contains("Apache License") && licence.contains("Version 2.0"));
}
