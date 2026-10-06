// The agent skill the app installs is the `skill/` folder at the top of the
// repository, the same files `pinrail docs` prints. This embeds them as
// they are installed: the SDK's version put into the brief that runs the
// dev shell, and a SHA-256 of all the files put into SKILL.md's front
// matter, so the app can tell an agent's copy is outdated by that alone.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo = manifest.join("../../..");
    let skill = repo.join("skill");
    println!("cargo:rerun-if-changed={}", skill.display());
    let sdk_package = repo.join("sdk/package.json");
    println!("cargo:rerun-if-changed={}", sdk_package.display());
    let sdk_version = version_of(&fs::read_to_string(&sdk_package).unwrap());

    let mut files = Vec::new();
    walk(&skill, &skill, &mut files);
    files.sort();
    let files: Vec<(String, String)> = files
        .into_iter()
        .map(|(rel, file)| {
            let text = fs::read_to_string(file).unwrap();
            let text = text.replace("{{sdk_version}}", &sdk_version);
            (rel, text)
        })
        .collect();

    // the files as they are written, SKILL.md with `{sha}` still in it
    let mut hash = Sha256::new();
    for (rel, text) in &files {
        hash.update(rel.as_bytes());
        hash.update([0]);
        hash.update(text.as_bytes());
        hash.update([0]);
    }
    let sha: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mut table = String::from("&[\n");
    for (rel, text) in &files {
        let text = if rel == "SKILL.md" {
            assert!(
                text.contains("{sha}"),
                "skill/SKILL.md has no {{sha}} in its front matter"
            );
            text.replace("{sha}", &sha)
        } else {
            text.clone()
        };
        let path = out.join("skill").join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        table.push_str(&format!(
            "    ({rel:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    table.push(']');
    fs::write(out.join("skill_files.rs"), table).unwrap();
    fs::write(out.join("skill_sha.rs"), format!("{sha:?}")).unwrap();

    tauri_build::build()
}

/// The `version` of a package.json, read without a JSON parser in the
/// build's dependencies.
fn version_of(package: &str) -> String {
    let after = package
        .split("\"version\"")
        .nth(1)
        .expect("the SDK's package.json has a version");
    after
        .split('"')
        .nth(1)
        .expect("the SDK's version is a string")
        .to_string()
}

/// Every file under `dir`, as its path in the skill, such as
/// `references/asking.md`, and its file.
fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
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
            walk(root, &path, out);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
            let rel = path.strip_prefix(root).unwrap();
            out.push((rel.to_string_lossy().replace('\\', "/"), path));
        }
    }
}
