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
    /// TypeScript in src/, without a framework, built by Vite into view/
    #[value(alias = "ts")]
    Typescript,
    /// React in src/, built by Vite into view/
    React,
}

impl Template {
    fn layer(self) -> &'static str {
        match self {
            Template::Plain => "plain",
            Template::Typescript => "typescript",
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
const TYPES: &str = include_str!("../../sdk/types.d.ts");

/// The SDK package's manifest, for the version a plugin's tests take.
const SDK_PACKAGE: &str = include_str!("../../sdk/package.json");

/// What a new plugin is written with.
pub struct Options<'a> {
    pub template: Template,
    /// tests under the SDK's harness, with Playwright
    pub playwright: bool,
    /// where the tests' package.json takes the SDK from, in place of its
    /// release of the version this command was built with
    pub sdk: Option<&'a str>,
}

/// The version of the SDK package this command was built with, which the
/// app it comes with serves.
pub fn sdk_version() -> String {
    let package: serde_json::Value =
        serde_json::from_str(SDK_PACKAGE).expect("the SDK's package.json is JSON");
    package["version"]
        .as_str()
        .expect("the SDK has a version")
        .to_string()
}

/// The SDK package of the version this command was built with, as the
/// tarball attached to its release.
fn sdk_dependency() -> String {
    let version = sdk_version();
    format!(
        "https://github.com/forgeplane/pinrail/releases/download/sdk-v{version}/pinrail-sdk-{version}.tgz"
    )
}

/// The files of one layer of the templates: each one's path in the plugin
/// folder, with the plugin's name still `__NAME__`, and its text. A
/// `_gitignore` is written as `.gitignore`, which the templates' own
/// folder cannot hold without ignoring files itself.
fn layer(name: &str) -> Vec<(String, String)> {
    let prefix = format!("{name}/");
    TEMPLATES
        .iter()
        .filter_map(|(path, text)| {
            let rel = path.strip_prefix(&prefix)?;
            let rel = match rel.strip_suffix("_gitignore") {
                Some(folder) if folder.is_empty() || folder.ends_with('/') => {
                    format!("{folder}.gitignore")
                }
                _ => rel.to_string(),
            };
            Some((rel, text.to_string()))
        })
        .collect()
}

/// The files a plugin gets: the common layer, the template's, and with
/// `--playwright` the testing layer, whose package.json, .gitignore and
/// README part are added to the template's when it has them.
fn files(options: &Options) -> Result<Vec<(String, String)>> {
    let mut out = layer("common");
    out.extend(layer(options.template.layer()));
    out.push(("pinrail-plugin.d.ts".to_string(), TYPES.to_string()));
    if options.playwright {
        for (rel, text) in layer("playwright") {
            match out.iter_mut().find(|(path, _)| *path == rel) {
                Some((_, base)) if rel == "package.json" => *base = with_tests(base, &text)?,
                Some((_, base)) if rel == ".gitignore" => {
                    let missing: Vec<&str> = text
                        .lines()
                        .filter(|l| !base.lines().any(|b| b == *l))
                        .collect();
                    for line in missing {
                        base.push_str(line);
                        base.push('\n');
                    }
                }
                Some((_, base)) => base.push_str(&text),
                None => out.push((rel, text)),
            }
        }
    }
    Ok(out)
}

/// A template's package.json with the testing layer's script and
/// development dependencies added. A view with a build is built first.
fn with_tests(base: &str, testing: &str) -> Result<String> {
    let mut base: serde_json::Value = serde_json::from_str(base)?;
    let testing: serde_json::Value = serde_json::from_str(testing)?;
    let test = testing["scripts"]["test"].as_str().unwrap_or_default();
    let scripts = &mut base["scripts"];
    scripts["test"] = if scripts.get("build").is_some() {
        format!("npm run build && {test}").into()
    } else {
        test.into()
    };
    if let (Some(dev), Some(added)) = (
        base["devDependencies"].as_object_mut(),
        testing["devDependencies"].as_object(),
    ) {
        dev.extend(added.clone());
    }
    Ok(serde_json::to_string_pretty(&base)? + "\n")
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
pub fn write(name: &str, dir: &Path, options: &Options) -> Result<Vec<PathBuf>> {
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
    let sdk = options
        .sdk
        .map(str::to_string)
        .unwrap_or_else(sdk_dependency);
    let mut written = Vec::new();
    for (to, text) in files(options)? {
        let to = to.replace("__NAME__", name);
        let target = dir.join(&to);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let text = text
            .replace("__NAME__", name)
            .replace("__TITLE__", &title)
            .replace("__SDK_DEP__", &sdk);
        std::fs::write(&target, text).with_context(|| format!("writing {}", target.display()))?;
        written.push(PathBuf::from(&to));
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> Options<'static> {
        Options {
            template: Template::Plain,
            playwright: false,
            sdk: None,
        }
    }

    #[test]
    fn a_new_plugin_is_whole_and_named_throughout() {
        let root = std::env::temp_dir().join(format!("pinrail-new-{}", std::process::id()));
        let dir = root.join("ticket_triage");
        let written = write("ticket_triage", &dir, &plain()).unwrap();
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
            write("ticket_triage", &dir, &plain())
                .unwrap_err()
                .to_string()
                .contains("is not empty")
        );
        assert!(write("Ticket", &root.join("x"), &plain()).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A view built by Vite, in TypeScript or React, typed by the SDK's
    /// types in the folder, so
    /// building it needs no SDK package.
    #[test]
    fn the_vite_and_react_templates_build_their_view_from_src() {
        let root = std::env::temp_dir().join(format!("pinrail-new-built-{}", std::process::id()));
        for (template, sources) in [
            (Template::Typescript, &["src/index.html", "src/main.ts"][..]),
            (
                Template::React,
                &["src/App.tsx", "src/index.html", "src/main.tsx"][..],
            ),
        ] {
            let dir = root.join(format!("{template:?}"));
            let written = write(
                "ticket_triage",
                &dir,
                &Options {
                    template,
                    playwright: false,
                    sdk: None,
                },
            )
            .unwrap();
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
                !read("package.json").contains("pinrail-sdk\":"),
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

    /// --playwright adds the tests, and joins what the template has: one
    /// package.json, one .gitignore, one README.
    #[test]
    fn playwright_adds_the_tests_to_any_template() {
        let root = std::env::temp_dir().join(format!("pinrail-new-tests-{}", std::process::id()));
        for template in [Template::Plain, Template::React] {
            let dir = root.join(format!("{template:?}"));
            let options = Options {
                template,
                playwright: true,
                sdk: None,
            };
            let written = write("ticket_triage", &dir, &options).unwrap();
            let names: Vec<_> = written
                .iter()
                .map(|f| f.to_string_lossy().into_owned())
                .collect();
            for file in [
                "package.json",
                ".gitignore",
                "playwright.config.ts",
                "tests/ticket_triage.spec.ts",
            ] {
                assert_eq!(
                    names.iter().filter(|n| *n == file).count(),
                    1,
                    "{template:?}: {file}"
                );
            }
            let read = |file: &str| std::fs::read_to_string(dir.join(file)).unwrap();
            let package: serde_json::Value = serde_json::from_str(&read("package.json")).unwrap();
            let dev = &package["devDependencies"];
            assert!(dev["@playwright/test"].is_string());
            let version =
                serde_json::from_str::<serde_json::Value>(SDK_PACKAGE).unwrap()["version"]
                    .as_str()
                    .unwrap()
                    .to_string();
            assert!(
                dev["pinrail-sdk"]
                    .as_str()
                    .unwrap()
                    .ends_with(&format!("sdk-v{version}/pinrail-sdk-{version}.tgz")),
                "{dev}"
            );
            let gitignore = read(".gitignore");
            assert_eq!(gitignore.matches("node_modules/").count(), 1, "{gitignore}");
            assert!(gitignore.contains("test-results/"));
            assert!(read("README.md").contains("## Tests\n"));
            assert!(read("tests/ticket_triage.spec.ts").contains("mountPlugin"));
            if template == Template::React {
                assert_eq!(
                    package["scripts"]["test"],
                    "npm run build && playwright test"
                );
                assert_eq!(package["scripts"]["build"], "tsc --noEmit && vite build");
                assert!(dev["vite"].is_string(), "the template's own stay");
            } else {
                assert_eq!(package["scripts"]["test"], "playwright test");
                assert_eq!(package["name"], "pinrail-plugin-ticket_triage");
            }
        }
        // the SDK from elsewhere, as the repository's own tests take it
        let dir = root.join("local");
        let options = Options {
            template: Template::Plain,
            playwright: true,
            sdk: Some("file:../sdk"),
        };
        write("ticket_triage", &dir, &options).unwrap();
        assert!(
            std::fs::read_to_string(dir.join("package.json"))
                .unwrap()
                .contains(r#""file:../sdk""#)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
