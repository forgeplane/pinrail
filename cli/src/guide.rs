//! `pinrail plugins guide`: the docs' pages on building a plugin, from the
//! Pinrail installed, offline. With no topic an index, a line a page; with
//! one, that page, made plain for a terminal: the site's own syntax turned
//! into markdown any reader takes.

/// Each page: its topic, and the docs' source.
const PAGES: &[(&str, &str)] = &[
    ("writing", include_str!("../../docs/building/writing.md")),
    ("design", include_str!("../../docs/building/design.md")),
    (
        "settings-and-keys",
        include_str!("../../docs/building/settings-and-keys.md"),
    ),
    ("protocol", include_str!("../../docs/building/protocol.md")),
    (
        "frameworks",
        include_str!("../../docs/building/frameworks.md"),
    ),
    (
        "publishing",
        include_str!("../../docs/building/publishing.md"),
    ),
];

const SITE: &str = "https://pinrail.dev";
const EXAMPLES: &str = "https://github.com/forgeplane/pinrail/blob/main/docs/examples";

/// The index: each topic with the page's title and what it covers.
pub fn index() -> String {
    let mut out = String::from(
        "# Building a Pinrail plugin\n\nThe guide, a topic at a time: `pinrail plugins guide <topic>`. Every SDK call, with its arguments, is in the SDK's types (`pinrail-plugin.d.ts`).\n\n",
    );
    for (topic, source) in PAGES {
        let (title, description) = front(source);
        out.push_str(&format!("- **{topic}**: {title}. {description}\n"));
    }
    out
}

/// The page for `topic`, or none for a topic the guide does not have.
pub fn page(topic: &str) -> Option<String> {
    PAGES
        .iter()
        .find(|(t, _)| *t == topic)
        .map(|(_, source)| plain(source))
}

pub fn topics() -> Vec<&'static str> {
    PAGES.iter().map(|(t, _)| *t).collect()
}

/// The page's `title` and `description`, from its front matter.
fn front(source: &str) -> (String, String) {
    let mut title = String::new();
    let mut description = String::new();
    if let Some(rest) = source.strip_prefix("---\n")
        && let Some((head, _)) = rest.split_once("\n---\n")
    {
        for line in head.lines() {
            if let Some(v) = line.strip_prefix("title:") {
                title = unquote(v);
            } else if let Some(v) = line.strip_prefix("description:") {
                description = unquote(v);
            }
        }
    }
    (title, description)
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

/// The page as plain markdown: the title as a heading, admonitions as bold
/// lines, screenshots gone, example files as links, code fences with their
/// language and their file named above them, and the site's links whole.
/// Inside a code fence nothing changes.
fn plain(source: &str) -> String {
    let (title, description) = front(source);
    let body = source
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(_, body)| body)
        .unwrap_or(source);
    let mut out = format!("# {title}\n\n{description}\n");
    let mut in_code = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(info) = trimmed.strip_prefix("```") {
            if in_code {
                in_code = false;
                out.push_str(line);
            } else {
                in_code = true;
                // ```html title="view/index.html" {3,5}: the file, then the fence
                let language = info.split_whitespace().next().unwrap_or("");
                if let Some(name) = info
                    .split("title=\"")
                    .nth(1)
                    .and_then(|r| r.split('"').next())
                {
                    out.push_str(&format!("`{name}`:\n\n"));
                }
                out.push_str(&format!("```{language}"));
            }
            out.push('\n');
            continue;
        }
        if in_code {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if let Some(admonition) = trimmed.strip_prefix(":::") {
            if admonition.is_empty() {
                continue;
            }
            let kind = admonition.split('[').next().unwrap_or("note");
            let label = admonition
                .split_once('[')
                .and_then(|(_, r)| r.strip_suffix(']'))
                .map(str::to_string)
                .unwrap_or_else(|| {
                    let mut k = kind.chars();
                    k.next()
                        .map(|c| c.to_uppercase().chain(k).collect())
                        .unwrap_or_default()
                });
            out.push_str(&format!("**{label}.** "));
            continue;
        }
        if trimmed.starts_with("![") && trimmed.contains("](screenshot:") {
            continue;
        }
        if trimmed.contains("](example:") {
            for (label, path) in embeds(trimmed) {
                out.push_str(&format!("- {label}: {EXAMPLES}/{path}\n"));
            }
            continue;
        }
        out.push_str(&line.replace("](/docs/", &format!("]({SITE}/docs/")));
        out.push('\n');
    }
    out
}

/// The `![Label](example:path)` embeds on a line, as (label, path).
fn embeds(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find("![") {
        let after = &rest[start + 2..];
        let Some((label, tail)) = after.split_once("](example:") else {
            break;
        };
        let Some((path, next)) = tail.split_once(')') else {
            break;
        };
        out.push((label.to_string(), path.to_string()));
        rest = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_prints_plain() {
        for topic in topics() {
            let page = page(topic).unwrap();
            assert!(page.starts_with("# "), "{topic}");
            let mut in_code = false;
            for line in page.lines() {
                if line.trim_start().starts_with("```") {
                    in_code = !in_code;
                    continue;
                }
                if in_code {
                    continue;
                }
                assert!(!line.trim_start().starts_with(":::"), "{topic}: {line}");
                assert!(!line.contains("](screenshot:"), "{topic}: {line}");
                assert!(!line.contains("](example:"), "{topic}: {line}");
                assert!(!line.contains("](/docs/"), "{topic}: {line}");
            }
        }
        assert!(index().contains("- **design**: Design and styling."));
    }

    #[test]
    fn the_site_syntax_becomes_markdown() {
        let page = plain(
            "---\ntitle: T\ndescription: \"D\"\n---\n\n:::tip[Check first]\nRun it.\n:::\n\n![x](screenshot:y \"z\")\nSee [the list](/docs/plugins/list/).\n\n```html title=\"view/index.html\" {3}\n:::you\n```\n![React](example:a/b.tsx) ![React](example:a/c.tsx)\n",
        );
        assert_eq!(
            page,
            "# T\n\nD\n\n**Check first.** Run it.\n\nSee [the list](https://pinrail.dev/docs/plugins/list/).\n\n`view/index.html`:\n\n```html\n:::you\n```\n- React: https://github.com/forgeplane/pinrail/blob/main/docs/examples/a/b.tsx\n- React: https://github.com/forgeplane/pinrail/blob/main/docs/examples/a/c.tsx\n"
        );
    }
}
