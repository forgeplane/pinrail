// The official plugins the app offers are plugins like any other: they live
// in `plugins/` with the test fixtures. The crate cannot reach outside itself
// for files it embeds, so their bundles are copied here at build time and
// embedded from there, as the catalog the person installs from.
// What a plugin ships is what an install would copy: the entries of the
// bundle layout, without hidden files.
//
// Every file in `migrations/` is embedded too, as a list in version order,
// so adding a migration is adding a file: `<version>_<name>.sql`, the
// version the UTC moment it was written, `YYYYMMDDHHMMSS`.

use std::path::{Path, PathBuf};
use std::{env, fs};

/// The top-level entries of a bundle, as `pinrail_format::bundle` lists
/// them; a test checks that the embedded copies match.
const LAYOUT: &[&str] = &[
    "manifest.json",
    "icon.svg",
    "README.md",
    "LICENSE",
    "schemas",
    "view",
    "templates",
    "samples",
];

/// The plugins the app's catalog offers, by their folder in `plugins/`.
const CATALOG: &[&str] = &["list", "feedback", "code-review", "image"];

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let catalog = out.join("catalog");

    migrations(&manifest.join("migrations"), &out.join("migrations.rs"));

    let _ = fs::remove_dir_all(&catalog);
    for name in CATALOG {
        let source = manifest.join("../../plugins").join(name);
        println!("cargo:rerun-if-changed={}", source.display());
        // a plugin with a build of its own has no view until it is built
        if !source.join("view/index.html").is_file() {
            panic!(
                "plugins/{name}/view/index.html is missing: build the plugins first, with `mise run plugins:build` or scripts/build-plugins.sh"
            );
        }
        copy(&source, &catalog.join(name), true);
    }
}

fn copy(from: &Path, to: &Path, top: bool) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || (top && !LAYOUT.contains(&name.as_ref())) {
            continue;
        }
        println!("cargo:rerun-if-changed={}", entry.path().display());
        if entry.path().is_dir() {
            copy(&entry.path(), &to.join(name.as_ref()), false);
        } else {
            fs::copy(entry.path(), to.join(name.as_ref())).unwrap();
        }
    }
}

/// Writes `(version, name, sql)` for every file in `dir`, oldest first. A
/// file whose name is not `<14 digits>_<name>.sql`, or a version used
/// twice, fails the build rather than being skipped.
fn migrations(dir: &Path, out: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut found: Vec<(u64, String, PathBuf)> = Vec::new();
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        if file.starts_with('.') {
            continue;
        }
        let parsed = file
            .strip_suffix(".sql")
            .and_then(|stem| stem.split_once('_'))
            .filter(|(version, name)| {
                version.len() == 14
                    && version.bytes().all(|b| b.is_ascii_digit())
                    && !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            });
        let Some((version, name)) = parsed else {
            panic!(
                "migrations/{file}: a migration is named <YYYYMMDDHHMMSS>_<snake_case_name>.sql"
            );
        };
        println!("cargo:rerun-if-changed={}", entry.path().display());
        found.push((version.parse().unwrap(), name.to_string(), entry.path()));
    }
    found.sort();
    for pair in found.windows(2) {
        assert!(
            pair[0].0 != pair[1].0,
            "migrations/: version {} is used twice",
            pair[0].0
        );
    }
    let rows: String = found
        .iter()
        .map(|(version, name, path)| {
            format!(
                "    ({version}, {name:?}, include_str!({:?})),\n",
                path.display().to_string()
            )
        })
        .collect();
    fs::write(out, format!("&[\n{rows}]\n")).unwrap();
}
