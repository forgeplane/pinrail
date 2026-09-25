// The briefs `pinrail docs` prints are the markdown files under `docs/`,
// embedded at build: this writes the table of them, path and source, so a
// new brief is a new file and nothing else.

use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("docs");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    walk(&root, &root, &mut files);
    files.sort();
    let mut table = String::from("&[\n");
    for (path, file) in &files {
        println!("cargo:rerun-if-changed={}", file.display());
        table.push_str(&format!(
            "    ({path:?}, include_str!({:?})),\n",
            file.display().to_string()
        ));
    }
    table.push(']');
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("briefs.rs");
    fs::write(out, table).unwrap();
}

/// Every `.md` under `dir`, as its path without the extension (`index` for
/// the root) and its file.
fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            println!("cargo:rerun-if-changed={}", path.display());
            walk(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            let rel = path.strip_prefix(root).unwrap().with_extension("");
            out.push((rel.to_string_lossy().replace('\\', "/"), path));
        }
    }
}
