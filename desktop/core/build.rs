// The built-in plugin is a plugin like any other: it lives in `plugins/list`
// with the rest. The crate cannot reach outside itself for files it embeds,
// so the bundle is copied here at build time and embedded from there. What a
// plugin ships is what the installer would copy: the manifest, the schemas
// and the view, without its tests, fixtures or readme.

use std::path::{Path, PathBuf};
use std::{env, fs};

/// What a bundle leaves behind, as `plugins/install.rs` does for an install.
const NOT_IN_THE_BUNDLE: &[&str] = &["node_modules", "src", "tests", "fixtures", "README.md"];

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let source = manifest.join("../../plugins/list");
    let target = PathBuf::from(env::var("OUT_DIR").unwrap()).join("builtin/list");

    println!("cargo:rerun-if-changed={}", source.display());
    let _ = fs::remove_dir_all(&target);
    copy(&source, &target);
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || NOT_IN_THE_BUNDLE.contains(&name.as_ref()) {
            continue;
        }
        println!("cargo:rerun-if-changed={}", entry.path().display());
        if entry.path().is_dir() {
            copy(&entry.path(), &to.join(name.as_ref()));
        } else {
            fs::copy(entry.path(), to.join(name.as_ref())).unwrap();
        }
    }
}
