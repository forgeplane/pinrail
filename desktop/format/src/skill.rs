//! A plugin's skill: the `skill/` folder beside its manifest, in the layout
//! agents read skills in, with `SKILL.md` and anything it refers to, such as
//! references and scripts. The app writes it into the `pinrail` skill of the
//! agents it connects, under `plugins/<name>/`, so an agent reads it before
//! it asks with the plugin.

use std::path::Path;

/// The folder a plugin's skill is in, beside its manifest.
pub const FOLDER: &str = "skill";
/// The skill's main file, inside its folder.
pub const MAIN: &str = "SKILL.md";
/// The most files a skill may hold.
pub const MAX_FILES: usize = 200;
/// The most a skill's files may add up to: 2 MB.
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;

/// A plugin's skill as it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// The front matter's `name`, or none.
    pub name: Option<String>,
    /// The front matter's `description`: what the skill tells an agent.
    pub description: String,
    /// The skill's files, relative to its folder with `/` between their
    /// parts, sorted, `SKILL.md` among them.
    pub files: Vec<String>,
}

/// The skill in a plugin's folder: none without a `skill/` folder, and why
/// it cannot be used when it is not in order.
pub fn load(dir: &Path) -> Result<Option<Skill>, String> {
    let folder = dir.join(FOLDER);
    let Ok(meta) = std::fs::symlink_metadata(&folder) else {
        return Ok(None);
    };
    if !meta.is_dir() {
        return Err(format!("{FOLDER} is not a folder"));
    }
    let mut files = Vec::new();
    let mut total = 0;
    walk(&folder, "", &mut files, &mut total)?;
    files.sort();
    if !files.iter().any(|f| f == MAIN) {
        return Err(format!("{FOLDER}/{MAIN} is missing"));
    }
    let text = std::fs::read_to_string(folder.join(MAIN))
        .map_err(|e| format!("{FOLDER}/{MAIN}: cannot read ({e})"))?;
    let (name, description) = front_matter(&text)?;
    Ok(Some(Skill {
        name,
        description,
        files,
    }))
}

/// Adds the files under `folder` to `files`, as paths below the skill's
/// folder, skipping hidden ones and refusing links and too much.
fn walk(
    folder: &Path,
    prefix: &str,
    files: &mut Vec<String>,
    total: &mut u64,
) -> Result<(), String> {
    let entries =
        std::fs::read_dir(folder).map_err(|e| format!("{FOLDER}/{prefix}: cannot read ({e})"))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // .DS_Store and the like are not part of the skill
        if name.starts_with('.') {
            continue;
        }
        let path = format!("{prefix}{name}");
        let meta = std::fs::symlink_metadata(entry.path())
            .map_err(|e| format!("{FOLDER}/{path}: cannot read ({e})"))?;
        if meta.file_type().is_symlink() {
            return Err(format!(
                "{FOLDER}/{path}: a link, which a skill does not hold"
            ));
        }
        if meta.is_dir() {
            walk(&entry.path(), &format!("{path}/"), files, total)?;
            continue;
        }
        files.push(path);
        *total += meta.len();
        if files.len() > MAX_FILES {
            return Err(format!("{FOLDER} holds more than {MAX_FILES} files"));
        }
        if *total > MAX_BYTES {
            return Err(format!(
                "{FOLDER}'s files add up to more than {MAX_BYTES} bytes"
            ));
        }
    }
    Ok(())
}

/// The `name` and `description` of a `SKILL.md`'s front matter, the lines
/// between its first two `---` lines.
fn front_matter(text: &str) -> Result<(Option<String>, String), String> {
    let missing = || format!("{FOLDER}/{MAIN} starts with no front matter between --- lines");
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
        .ok_or_else(missing)?;
    let end = rest.find("\n---").ok_or_else(missing)?;
    let mut name = None;
    let mut description = None;
    for line in rest[..end].lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = unquote(value.trim());
        match key.trim() {
            "name" if !value.is_empty() => name = Some(value),
            "description" if !value.is_empty() => description = Some(value),
            _ => {}
        }
    }
    let description =
        description.ok_or_else(|| format!("{FOLDER}/{MAIN}'s front matter has no description"))?;
    Ok((name, description))
}

/// A front matter value without the quotes around it, if it has them.
fn unquote(value: &str) -> String {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|v| v.strip_suffix(quote))
        {
            return inner.to_string();
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, path: &str, text: &str) {
        let path = dir.join(FOLDER).join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    const MAIN_TEXT: &str =
        "---\nname: change-map\ndescription: \"How to map a change\"\n---\n\n# Change map\n";

    #[test]
    fn a_plugin_without_a_skill_folder_has_no_skill() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()), Ok(None));
    }

    #[test]
    fn a_skill_is_its_front_matter_and_its_files_in_order() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), MAIN, MAIN_TEXT);
        write(dir.path(), "scripts/files.mjs", "// lists the diff");
        write(dir.path(), "references/payload.md", "# The payload");
        write(dir.path(), ".DS_Store", "");
        let skill = load(dir.path()).unwrap().unwrap();
        assert_eq!(skill.name.as_deref(), Some("change-map"));
        assert_eq!(skill.description, "How to map a change");
        assert_eq!(
            skill.files,
            ["SKILL.md", "references/payload.md", "scripts/files.mjs"]
        );
    }

    #[test]
    fn a_skill_needs_its_main_file_with_a_description() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "notes.md", "# Notes");
        assert_eq!(
            load(dir.path()),
            Err("skill/SKILL.md is missing".to_string())
        );
        write(dir.path(), MAIN, "# No front matter");
        assert!(load(dir.path()).unwrap_err().contains("no front matter"));
        write(dir.path(), MAIN, "---\nname: x\n---\n");
        assert_eq!(
            load(dir.path()),
            Err("skill/SKILL.md's front matter has no description".to_string())
        );
    }

    #[test]
    fn a_skill_holds_no_links() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), MAIN, MAIN_TEXT);
        crate::file_link("/etc/hosts", dir.path().join(FOLDER).join("hosts"));
        assert_eq!(
            load(dir.path()),
            Err("skill/hosts: a link, which a skill does not hold".to_string())
        );
    }

    #[test]
    fn a_skill_is_kept_small() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), MAIN, MAIN_TEXT);
        write(dir.path(), "big.bin", &"x".repeat(MAX_BYTES as usize + 1));
        assert!(
            load(dir.path())
                .unwrap_err()
                .contains("add up to more than")
        );
    }
}
