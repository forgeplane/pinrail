// The briefs `pinrail docs` prints are the Markdown files of the agent
// skill, in `skill/` at the top of the repository: its SKILL.md and its
// references. They are embedded at build: this writes the table of them,
// each file's path in the skill and its source, so a new brief is a new
// file and nothing else.

use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../skill");
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

/// Every `.md` under `dir`, as its path in the skill, such as
/// `references/asking.md`, and its file.
fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            println!("cargo:rerun-if-changed={}", path.display());
            walk(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            let rel = path.strip_prefix(root).unwrap();
            out.push((rel.to_string_lossy().replace('\\', "/"), path));
        }
    }
}
