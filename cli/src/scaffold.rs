//! `pinrail plugins new`: a plugin folder from the templates in
//! `templates/`, embedded at build. Every plugin gets the `common` layer,
//! then the layer of its template.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;

/// How the view is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Template {
    /// HTML and JavaScript in view/, with no build
    Plain,
    /// TypeScript in src/, built by Vite into view/
    Vite,
    /// React in src/, built by Vite into view/
    React,
}

impl Template {
    fn layer(self) -> &'static str {
        match self {
            Template::Plain => "plain",
            Template::Vite => "vite",
            Template::React => "react",
        }
    }

    /// Whether the view is built from src/ before the app can serve it.
    pub fn builds(self) -> bool {
        self != Template::Plain
    }
}

/// Each file of the templates: its path under `templates/`, such as
/// `plain/view/view.js`, and its text.
const TEMPLATES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/templates.rs"));

/// The SDK's types, beside a view without a build, for its editor.
const TYPES: &str = include_str!("../../pinrail-plugin/types.d.ts");

/// The files a template writes: each one's path in the plugin folder, with
/// the plugin's name still `__NAME__`, and its text. A template's
/// `_gitignore` is written as `.gitignore`, which the templates' own
/// folder cannot hold without ignoring files itself.
fn files(template: Template) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for layer in ["common", template.layer()] {
        let prefix = format!("{layer}/");
        for (path, text) in TEMPLATES {
            if let Some(rel) = path.strip_prefix(&prefix) {
                let rel = match rel.strip_suffix("_gitignore") {
                    Some(folder) if folder.is_empty() || folder.ends_with('/') => {
                        format!("{folder}.gitignore")
                    }
                    _ => rel.to_string(),
                };
                out.push((rel, *text));
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
pub fn write(name: &str, dir: &Path, template: Template) -> Result<Vec<PathBuf>> {
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
    for (to, text) in files(template) {
        let to = to.replace("__NAME__", name);
        let target = dir.join(&to);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let text = text.replace("__NAME__", name).replace("__TITLE__", &title);
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
        let written = write("ticket_triage", &dir, Template::Plain).unwrap();
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
            write("ticket_triage", &dir, Template::Plain)
                .unwrap_err()
                .to_string()
                .contains("is not empty")
        );
        assert!(write("Ticket", &root.join("x"), Template::Plain).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A view built by Vite, typed by the SDK's types in the folder, so
    /// building it needs no SDK package.
    #[test]
    fn the_vite_and_react_templates_build_their_view_from_src() {
        let root = std::env::temp_dir().join(format!("pinrail-new-built-{}", std::process::id()));
        for (template, sources) in [
            (Template::Vite, &["src/index.html", "src/main.ts"][..]),
            (
                Template::React,
                &["src/App.tsx", "src/index.html", "src/main.tsx"][..],
            ),
        ] {
            let dir = root.join(format!("{template:?}"));
            let written = write("ticket_triage", &dir, template).unwrap();
            let mut names: Vec<_> = written
                .iter()
                .map(|f| f.to_string_lossy().into_owned())
                .collect();
            names.sort();
            let mut expected = vec![
                ".gitignore",
                "README.md",
                "icon.svg",
                "manifest.json",
                "package.json",
                "pinrail-plugin.d.ts",
                "samples/ticket_triage.json",
                "schemas/decision.schema.json",
                "schemas/payload.schema.json",
                "tsconfig.json",
                "vite.config.ts",
            ];
            expected.extend(sources);
            expected.sort();
            assert_eq!(names, expected, "{template:?}");
            assert!(!dir.join("view").exists(), "the build writes view/");

            let read = |file: &str| std::fs::read_to_string(dir.join(file)).unwrap();
            for file in &names {
                assert!(
                    !read(file).contains("__NAME__") && !read(file).contains("__TITLE__"),
                    "{file}"
                );
            }
            let package: serde_json::Value = serde_json::from_str(&read("package.json")).unwrap();
            assert_eq!(package["name"], "pinrail-plugin-ticket_triage");
            assert!(package["scripts"]["build"].is_string());
            assert!(
                !read("package.json").contains("pinrail-plugin\":"),
                "no SDK package"
            );
            let tsconfig: serde_json::Value = serde_json::from_str(&read("tsconfig.json")).unwrap();
            assert_eq!(
                tsconfig["include"],
                serde_json::json!(["src", "pinrail-plugin.d.ts"])
            );
            let main = if template == Template::React {
                "src/App.tsx"
            } else {
                "src/main.ts"
            };
            assert!(read(main).contains(r#"from "../pinrail-plugin""#), "{main}");
            assert!(read(".gitignore").contains("/view/"));
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
