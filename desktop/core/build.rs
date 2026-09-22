// The built-in plugins are plugins like any other: they live in `plugins/`
// with the rest. The crate cannot reach outside itself for files it embeds,
// so their bundles are copied here at build time and embedded from there.
// What a plugin ships is what the installer would copy: the manifest, the
// schemas and the view, without its tests, fixtures or readme.

use std::path::{Path, PathBuf};
use std::{env, fs};

/// What a bundle leaves behind, as `plugins/install.rs` does for an install.
const NOT_IN_THE_BUNDLE: &[&str] = &["node_modules", "src", "tests", "fixtures", "README.md"];

/// The plugins every server has, by their folder in `plugins/`.
const BUILTIN: &[&str] = &["list", "feedback"];

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let builtin = PathBuf::from(env::var("OUT_DIR").unwrap()).join("builtin");

    let _ = fs::remove_dir_all(&builtin);
    for name in BUILTIN {
        let source = manifest.join("../../plugins").join(name);
        println!("cargo:rerun-if-changed={}", source.display());
        copy(&source, &builtin.join(name));
    }
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
