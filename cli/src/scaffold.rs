//! `pinrail plugins new`: a plugin that needs no build and no npm, from the
//! SDK's own `plain` and `common` templates, so the two scaffolds cannot
//! drift. What needs Node is left out: the package, the Playwright harness,
//! the release workflow, and the fixtures the sample stands in for.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Each file: where it lands, and the template it comes from.
const FILES: &[(&str, &str)] = &[
    (
        "manifest.json",
        include_str!("../../pinrail-plugin/templates/plain/manifest.json"),
    ),
    (
        "view/index.html",
        include_str!("../../pinrail-plugin/templates/plain/view/index.html"),
    ),
    (
        "view/view.js",
        include_str!("../../pinrail-plugin/templates/plain/view/view.js"),
    ),
    (
        "schemas/payload.schema.json",
        include_str!("../../pinrail-plugin/templates/common/schemas/payload.schema.json"),
    ),
    (
        "schemas/decision.schema.json",
        include_str!("../../pinrail-plugin/templates/common/schemas/decision.schema.json"),
    ),
    (
        "example.json",
        include_str!("../../pinrail-plugin/templates/common/example.json"),
    ),
    (
        "sample.json",
        include_str!("../../pinrail-plugin/templates/common/sample.json"),
    ),
    (
        "AGENTS.md",
        include_str!("../../pinrail-plugin/templates/common/AGENTS.md"),
    ),
    (
        "CLAUDE.md",
        include_str!("../../pinrail-plugin/templates/common/CLAUDE.md"),
    ),
    (
        "pinrail-plugin.d.ts",
        include_str!("../../pinrail-plugin/types.d.ts"),
    ),
    ("README.md", README),
];

const README: &str = r#"# __TITLE__

A Pinrail plugin: what an agent asks (`schemas/payload.schema.json`), what
the person answers (`schemas/decision.schema.json`), and the view between
them (`view/index.html` and `view/view.js`). It starts as one yes-or-no
question with a comment; make it yours from there.

```sh
pinrail plugins install . --link    # the app serves this folder live
pinrail submit __NAME__ --sample    # send it its sample; the review opens in the app
pinrail plugins reload              # after changing the manifest or a schema
pinrail plugins check .             # what the app would refuse, and why
pinrail plugins guide               # how to build a plugin, a topic at a time
```

`AGENTS.md` explains the plugin to an agent helping you build it.

For a view in React, Vue or Svelte, or tests that run without the app, start
from the npm package instead: `npx @forgeplane/pinrail-plugin create`.
"#;

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
    for (to, template) in FILES {
        let target = dir.join(to);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let text = template
            .replace("__NAME__", name)
            .replace("__TITLE__", &title);
        std::fs::write(&target, text).with_context(|| format!("writing {}", target.display()))?;
        written.push(PathBuf::from(to));
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
        assert_eq!(written.len(), FILES.len());
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
        assert_eq!(manifest["sample"], "sample.json");
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
