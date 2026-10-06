//! `pinrail docs`: short briefs for an agent, built into the command from
//! the agent skill in `skill/`, the same files the app installs into an
//! agent's skills. The root is the skill's SKILL.md, which says what
//! Pinrail is and how to use it; the others are its references. Each file
//! ends with a `## More` list of links to the files under it, so an agent
//! reads the least it needs and opens a branch only when its task does.
//! Printed here, every link to another file becomes the command that
//! prints it.

use serde_json::{Value, json};

/// Each file of the skill: its path in the skill, such as
/// `references/asking.md`, and its source.
const FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/briefs.rs"));

pub struct Brief {
    /// `asking`, `plugins/building`, or `index` for the root
    pub path: String,
    /// where the file is in the skill, which its links are relative to
    file: &'static str,
    pub title: String,
    /// the line its parent's menu gives it; the skill's description for the root
    pub summary: String,
    /// the children, in the order the menu shows them, each with its line
    pub menu: Vec<(String, String)>,
    pub body: &'static str,
}

/// The brief's path for a file of the skill.
fn brief_path(file: &str) -> String {
    if file == "SKILL.md" {
        return "index".to_string();
    }
    let path = file.strip_prefix("references/").unwrap_or(file);
    path.strip_suffix(".md").unwrap_or(path).to_string()
}

/// The file a link in `from` points to, as a path in the skill, if it is
/// a relative link to one of the skill's Markdown files. A heading in the
/// file, after `#`, does not count: `pinrail docs` prints the whole file.
fn resolve(from: &str, target: &str) -> Option<String> {
    let target = target.split('#').next().unwrap_or(target);
    if !target.ends_with(".md") || target.contains("://") || target.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for part in target.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// The skill's `description`, from its front matter.
fn description(head: &str) -> String {
    head.lines()
        .find_map(|line| line.strip_prefix("description:"))
        .map(|d| d.trim().trim_matches('"').to_string())
        .unwrap_or_default()
}

/// The links of the `## More` list at the end of a body, each as the file
/// it points to and the text after it.
fn menu(file: &str, body: &str) -> Vec<(String, String)> {
    let Some((_, more)) = body.split_once("\n## More\n") else {
        return Vec::new();
    };
    more.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("- [")?;
            let (_, rest) = rest.split_once("](")?;
            let (target, rest) = rest.split_once(')')?;
            let summary = rest.strip_prefix(':').unwrap_or(rest).trim();
            Some((brief_path(&resolve(file, target)?), summary.to_string()))
        })
        .collect()
}

fn parse(file: &'static str, source: &'static str) -> Brief {
    let (head, body) = source
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .unwrap_or(("", source));
    let body = body.trim_start_matches('\n');
    Brief {
        path: brief_path(file),
        file,
        title: body
            .lines()
            .find_map(|l| l.strip_prefix("# "))
            .unwrap_or_default()
            .to_string(),
        summary: description(head),
        menu: menu(file, body),
        body,
    }
}

pub fn all() -> Vec<Brief> {
    let mut all: Vec<Brief> = FILES
        .iter()
        .map(|(file, source)| parse(file, source))
        .collect();
    // a child's summary is the line its parent's menu gives it
    let lines: Vec<(String, String)> = all.iter().flat_map(|b| b.menu.clone()).collect();
    for brief in &mut all {
        if let Some((_, line)) = lines.iter().find(|(p, _)| *p == brief.path) {
            brief.summary = line.clone();
        }
    }
    all
}

/// The brief at `path` (none or empty for the root).
pub fn find(path: Option<&str>) -> Option<Brief> {
    let wanted = path
        .map(|p| p.trim_matches('/'))
        .filter(|p| !p.is_empty())
        .unwrap_or("index");
    all().into_iter().find(|b| b.path == wanted)
}

/// The command that prints a brief.
fn command(path: &str) -> String {
    if path == "index" {
        "pinrail docs".to_string()
    } else {
        format!("pinrail docs {path}")
    }
}

/// The body with each link to another file of the skill, `[text](file.md)`,
/// replaced by the command that prints that file.
fn commands_for_links(file: &str, body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(open) = rest.find('[') {
        let link = rest[open + 1..].split_once("](").and_then(|(text, after)| {
            let (target, _) = after.split_once(')')?;
            let path = brief_path(&resolve(file, target)?);
            let len = 1 + text.len() + 2 + target.len() + 1;
            (!text.contains(['[', '\n'])).then_some((path, len))
        });
        match link {
            Some((path, len)) => {
                out.push_str(&rest[..open]);
                out.push_str(&format!("`{}`", command(&path)));
                rest = &rest[open + len..];
            }
            None => {
                out.push_str(&rest[..=open]);
                rest = &rest[open + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The brief as it prints: its text, the links to other briefs as the
/// commands that print them.
pub fn render(brief: &Brief) -> String {
    let body = brief
        .body
        .trim_end()
        .replace("{{sdk_version}}", &crate::scaffold::sdk_version());
    let mut out = commands_for_links(brief.file, &body);
    out.push('\n');
    out
}

pub fn to_json(brief: &Brief) -> Value {
    json!({
        "path": if brief.path == "index" { "" } else { brief.path.as_str() },
        "title": brief.title,
        "summary": brief.summary,
        "markdown": render(brief),
        "children": brief.menu.iter().map(|(path, summary)| json!({ "path": path, "summary": summary })).collect::<Vec<_>>(),
    })
}

/// Every path and its line, indented by depth, from the root down.
pub fn tree() -> String {
    fn walk(brief: &Brief, depth: usize, out: &mut String) {
        out.push_str(&format!(
            "{}{}: {}\n",
            "  ".repeat(depth),
            command(&brief.path),
            brief.summary
        ));
        for (child, _) in &brief.menu {
            if let Some(child) = find(Some(child)) {
                walk(&child, depth + 1, out);
            }
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
    let near: Vec<String> = FILES
        .iter()
        .map(|(file, _)| brief_path(file))
        .filter(|p| p != "index" && words.iter().any(|w| p.contains(w)))
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
            // the text an agent reads, without the menu at the end
            let text = brief.body.split("\n## More\n").next().unwrap();
            let limit = if brief.path == "index" { 8192 } else { 20480 };
            assert!(
                text.len() <= limit,
                "{} is {} bytes, over {limit}",
                brief.path,
                text.len()
            );
            for (child, _) in &brief.menu {
                assert!(
                    find(Some(child)).is_some(),
                    "{}: its menu names {child}, which is not there",
                    brief.path
                );
            }
            if brief.path != "index" {
                assert!(
                    all.iter()
                        .any(|b| b.menu.iter().any(|(c, _)| *c == brief.path)),
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
    fn a_link_to_another_brief_prints_as_its_command() {
        let root = render(&find(None).unwrap());
        assert!(
            root.contains("`pinrail docs building` describes."),
            "{root}"
        );
        assert!(!root.contains("](references/"), "{root}");
        // a link to a heading in the same brief stays as it is
        assert!(
            root.contains("[choosing a plugin](#choosing-a-plugin)"),
            "{root}"
        );
        assert_eq!(
            find(Some("building")).unwrap().summary,
            "Make a plugin: create it, design its decision, work on its view with the person, test it, and try it in the app."
        );
        assert_eq!(
            resolve("references/building.md", "../SKILL.md").as_deref(),
            Some("SKILL.md")
        );
        assert_eq!(
            resolve("SKILL.md", "references/building.md#8-test-it").as_deref(),
            Some("references/building.md")
        );
        assert_eq!(
            commands_for_links(
                "SKILL.md",
                "[a] b [c](https://x.md) [d](references/building.md)"
            ),
            "[a] b [c](https://x.md) `pinrail docs building`"
        );
    }

    /// The dev shell is run at the SDK's version this command was built
    /// with, so the view behaves there as in the app.
    #[test]
    fn the_dev_shell_brief_names_the_sdk_version_of_this_build() {
        let brief = render(&find(Some("building")).unwrap());
        let command = format!(
            "npx pinrail-sdk@{} dev <path> --no-open",
            crate::scaffold::sdk_version()
        );
        assert!(brief.contains(&command), "{brief}");
        for brief in all() {
            assert!(
                !render(&brief).contains("{{"),
                "{}: a placeholder left",
                brief.path
            );
        }
    }

    #[test]
    fn the_root_ends_with_its_menu() {
        let root = render(&find(None).unwrap());
        assert!(
            root.contains("## More\n\n- `pinrail docs building`: "),
            "{root}"
        );
        assert!(tree().lines().next().unwrap().starts_with("pinrail docs: "));
        assert!(not_found("build").contains("nearest: building"));
    }
}
