// Two folders are embedded in the command at build, each as a table of its
// files, so a new file in either needs nothing else:
//
// - The briefs `pinrail docs` prints are the Markdown files of the agent
//   skill, in `skill/` at the top of the repository: its SKILL.md and its
//   references.
// - The templates `pinrail plugins new` writes are in `templates/`, in
//   layers: `common`, which every plugin gets, then one per template.

use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    table(
        &manifest.join("../skill"),
        Some("md"),
        &out.join("briefs.rs"),
    );
    table(&manifest.join("templates"), None, &out.join("templates.rs"));
}

/// Writes the table of the files under `root` to `out`: each one's path
/// under `root` and its contents, the files of one extension or all.
fn table(root: &Path, extension: Option<&str>, out: &Path) {
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    walk(root, root, extension, &mut files);
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
    fs::write(out, table).unwrap();
}

/// Every file under `dir` with the extension, or every file, as its path
/// under `root`, such as `references/asking.md`, and its file.
fn walk(root: &Path, dir: &Path, extension: Option<&str>, out: &mut Vec<(String, PathBuf)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        // a file the system or an editor left, such as .DS_Store
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if path.is_dir() {
            println!("cargo:rerun-if-changed={}", path.display());
            walk(root, &path, extension, out);
        } else if extension.is_none_or(|e| path.extension().is_some_and(|x| x == e)) {
            let rel = path.strip_prefix(root).unwrap();
            out.push((rel.to_string_lossy().replace('\\', "/"), path));
        }
    }
}
