//! `pinrail docs`: short briefs for an agent, like skills, built into the
//! command from `cli/docs/`. The root says what Pinrail is and how to use
//! it; every brief ends with a menu of the briefs under it, so an agent
//! reads the least it needs and opens a branch only when its task does.

use serde_json::{Value, json};

/// Each brief: its path (`asking/rounds`, `index` for the root) and source.
const BRIEFS: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/briefs.rs"));

pub struct Brief {
    pub path: &'static str,
    pub title: String,
    pub summary: String,
    /// the children, in the order the menu shows them
    pub menu: Vec<String>,
    pub body: &'static str,
}

fn parse(path: &'static str, source: &'static str) -> Brief {
    let (head, body) = source
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .unwrap_or(("", source));
    let mut brief = Brief {
        path,
        title: String::new(),
        summary: String::new(),
        menu: Vec::new(),
        body: body.trim_start_matches('\n'),
    };
    for line in head.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        match key.trim() {
            "title" => brief.title = value.to_string(),
            "summary" => brief.summary = value.to_string(),
            "menu" => {
                brief.menu = value
                    .trim_matches(['[', ']'])
                    .split(',')
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .map(|c| {
                        if path == "index" {
                            c.to_string()
                        } else {
                            format!("{path}/{c}")
                        }
                    })
                    .collect()
            }
            _ => {}
        }
    }
    brief
}

pub fn all() -> Vec<Brief> {
    BRIEFS
        .iter()
        .map(|(path, source)| parse(path, source))
        .collect()
}

/// The brief at `path` (none or empty for the root).
pub fn find(path: Option<&str>) -> Option<Brief> {
    let wanted = path
        .map(|p| p.trim_matches('/'))
        .filter(|p| !p.is_empty())
        .unwrap_or("index");
    all().into_iter().find(|b| b.path == wanted)
}

/// The manifest's JSON Schema, as the SDK ships it and the core checks it.
const MANIFEST_SCHEMA: &str = include_str!("../../pinrail-plugin/schemas/manifest.schema.json");

/// The brief as it prints: its text, then the menu of what is under it.
pub fn render(brief: &Brief) -> String {
    let mut out = brief
        .body
        .trim_end()
        .replace("{{manifest_schema}}", MANIFEST_SCHEMA.trim_end());
    out.push('\n');
    let children: Vec<Brief> = brief.menu.iter().filter_map(|p| find(Some(p))).collect();
    if !children.is_empty() {
        out.push_str("\n## More\n\n");
        for child in &children {
            out.push_str(&format!(
                "- `pinrail docs {}`: {}\n",
                child.path, child.summary
            ));
        }
    }
    out
}

pub fn to_json(brief: &Brief) -> Value {
    json!({
        "path": if brief.path == "index" { "" } else { brief.path },
        "title": brief.title,
        "summary": brief.summary,
        "markdown": render(brief),
        "children": brief.menu.iter().filter_map(|p| find(Some(p))).map(|c| json!({ "path": c.path, "summary": c.summary })).collect::<Vec<_>>(),
    })
}

/// Every path and its line, indented by depth, from the root down.
pub fn tree() -> String {
    fn walk(brief: &Brief, depth: usize, out: &mut String) {
        let name = if brief.path == "index" {
            "pinrail docs".to_string()
        } else {
            format!("pinrail docs {}", brief.path)
        };
        out.push_str(&format!(
            "{}{name}: {}\n",
            "  ".repeat(depth),
            brief.summary
        ));
        for child in brief.menu.iter().filter_map(|p| find(Some(p))) {
            walk(&child, depth + 1, out);
        }
    }
    let mut out = String::new();
    if let Some(root) = find(None) {
        walk(&root, 0, &mut out);
    }
    out
}

/// What to say for a path that is not there: the paths that share a word
/// with it, and the root's menu.
pub fn not_found(path: &str) -> String {
    let words: Vec<&str> = path
        .split(['/', '-', ' '])
        .filter(|w| !w.is_empty())
        .collect();
    let near: Vec<&str> = BRIEFS
        .iter()
        .map(|(p, _)| *p)
        .filter(|p| *p != "index" && words.iter().any(|w| p.contains(w)))
        .collect();
    let mut out = format!("no brief at {path:?}");
    if !near.is_empty() {
        out.push_str(&format!("; nearest: {}", near.join(", ")));
    }
    out.push_str(". `pinrail docs --tree` lists them all.");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn every_brief_loads_is_brief_and_is_on_a_menu() {
        let all = all();
        assert!(find(None).is_some(), "the root");
        for brief in &all {
            assert!(
                !brief.title.is_empty() && !brief.summary.is_empty(),
                "{}: title and summary",
                brief.path
            );
            let limit = if brief.path == "index" { 2048 } else { 4096 };
            assert!(
                brief.body.len() <= limit,
                "{} is {} bytes, over {limit}",
                brief.path,
                brief.body.len()
            );
            for child in &brief.menu {
                assert!(
                    find(Some(child)).is_some(),
                    "{}: its menu names {child}, which is not there",
                    brief.path
                );
            }
            if brief.path != "index" {
                assert!(
                    all.iter().any(|b| b.menu.iter().any(|c| c == brief.path)),
                    "{} is on no menu",
                    brief.path
                );
            }
        }
    }

    /// Every `pinrail …` a brief shows in a code block parses with the
    /// command's own definition, so a renamed flag fails here first.
    #[test]
    fn every_command_a_brief_shows_parses() {
        for brief in all() {
            let mut in_code = false;
            let mut pending = String::new();
            for line in brief.body.lines() {
                let t = line.trim();
                if t.starts_with("```") {
                    in_code = !in_code;
                    continue;
                }
                if !in_code {
                    continue;
                }
                let t = t.split(" # ").next().unwrap_or(t).trim_end();
                if pending.is_empty() && !t.starts_with("pinrail ") && !t.contains(" pinrail ") {
                    continue;
                }
                let t = match t.find("pinrail ") {
                    Some(at) if pending.is_empty() => &t[at..],
                    _ => t,
                };
                if let Some(head) = t.strip_suffix('\\') {
                    pending.push_str(head);
                    pending.push(' ');
                    continue;
                }
                pending.push_str(t);
                let command = std::mem::take(&mut pending);
                // up to a pipe, a redirect or the next command, spaces around it
                let command = [" || ", " | ", " > ", "; ", " && "]
                    .iter()
                    .fold(command.as_str(), |c, sep| c.split(sep).next().unwrap())
                    .trim();
                let words = shell_words(command);
                assert!(
                    crate::Cli::try_parse_from(&words).is_ok(),
                    "{}: `{command}` does not parse: {}",
                    brief.path,
                    crate::Cli::try_parse_from(&words).err().unwrap()
                );
            }
        }
    }

    /// Words as a shell splits them, quotes kept together.
    fn shell_words(line: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut word = String::new();
        let mut quote = None;
        for c in line.chars() {
            match (quote, c) {
                (None, '"' | '\'') => quote = Some(c),
                (Some(q), c) if c == q => quote = None,
                (None, c) if c.is_whitespace() => {
                    if !word.is_empty() {
                        words.push(std::mem::take(&mut word));
                    }
                }
                _ => word.push(c),
            }
        }
        if !word.is_empty() {
            words.push(word);
        }
        words
    }

    #[test]
    fn the_root_ends_with_its_menu() {
        let root = render(&find(None).unwrap());
        assert!(
            root.contains("## More\n\n- `pinrail docs asking`: "),
            "{root}"
        );
        assert!(tree().lines().next().unwrap().starts_with("pinrail docs: "));
        assert!(not_found("askin").contains("nearest: asking"));
    }
}
