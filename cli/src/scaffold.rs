//! `pinrail plugins new`: a plugin folder from the templates in
//! `templates/`, embedded at build. Every plugin gets the `common` layer,
//! then the layer of its template.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Each file of the templates: its path under `templates/`, such as
/// `plain/view/view.js`, and its text.
const TEMPLATES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/templates.rs"));

/// The SDK's types, beside a view without a build, for its editor.
const TYPES: &str = include_str!("../../pinrail-plugin/types.d.ts");

/// The files a template writes: each one's path in the plugin folder, with
/// the plugin's name still `__NAME__`, and its text.
fn files(template: &str) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for layer in ["common", template] {
        let prefix = format!("{layer}/");
        for (path, text) in TEMPLATES {
            if let Some(rel) = path.strip_prefix(&prefix) {
                out.push((rel.to_string(), *text));
            }
        }
    }
    out.push(("pinrail-plugin.d.ts".to_string(), TYPES));
    out
}

/// A plugin's name, as the manifest takes it.
fn valid(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// `ticket_triage` as "Ticket triage", as the SDK's `create` titles it.
pub fn title_of(name: &str) -> String {
    let spaced = name.replace('_', " ");
    let mut chars = spaced.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Writes the plugin into `dir`, which must be new or empty; the files
/// written, relative to it.
pub fn write(name: &str, dir: &Path) -> Result<Vec<PathBuf>> {
    if !valid(name) {
        bail!(
            "a plugin's name is a lowercase letter, then lowercase letters, digits, _ or -: {name:?}"
        );
    }
    if dir.exists()
        && std::fs::read_dir(dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .next()
            .is_some()
    {
        bail!("{} exists and is not empty", dir.display());
    }
    let title = title_of(name);
    let mut written = Vec::new();
    for (to, template) in files("plain") {
        let to = to.replace("__NAME__", name);
        let target = dir.join(&to);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let text = template
            .replace("__NAME__", name)
            .replace("__TITLE__", &title);
        std::fs::write(&target, text).with_context(|| format!("writing {}", target.display()))?;
        written.push(PathBuf::from(&to));
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_plugin_is_whole_and_named_throughout() {
        let root = std::env::temp_dir().join(format!("pinrail-new-{}", std::process::id()));
        let dir = root.join("ticket_triage");
        let written = write("ticket_triage", &dir).unwrap();
        let mut names: Vec<_> = written
            .iter()
            .map(|f| f.to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "README.md",
                "icon.svg",
                "manifest.json",
                "pinrail-plugin.d.ts",
                "samples/ticket_triage.json",
                "schemas/decision.schema.json",
                "schemas/payload.schema.json",
                "view/icons/LICENSE",
                "view/icons/check.svg",
                "view/icons/x.svg",
                "view/index.html",
                "view/view.js",
            ]
        );
        for file in &written {
            let text = std::fs::read_to_string(dir.join(file)).unwrap();
            assert!(
                !text.contains("__NAME__") && !text.contains("__TITLE__"),
                "{}",
                file.display()
            );
        }
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["name"], "ticket_triage");
        assert_eq!(manifest["title"], "Ticket triage");
        // the sample, named for the plugin, is where the app finds it
        assert!(manifest.get("sample").is_none());
        assert!(dir.join("samples/ticket_triage.json").is_file());
        // the icons are Lucide's, whose licence travels with them
        assert!(
            std::fs::read_to_string(dir.join("view/icons/LICENSE"))
                .unwrap()
                .starts_with("ISC License")
        );
        assert!(
            std::fs::read_to_string(dir.join("view/index.html"))
                .unwrap()
                .contains(r#"<script src="view.js"></script>"#)
        );

        assert!(
            write("ticket_triage", &dir)
                .unwrap_err()
                .to_string()
                .contains("is not empty")
        );
        assert!(write("Ticket", &root.join("x")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
