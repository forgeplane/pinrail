//! The skill and the docs name `pinrail items` commands and options. Each
//! one they name must exist, so that the text an agent and a person read
//! cannot drift from the CLI.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// The pages that teach `pinrail items`, from the repository's root.
const PAGES: &[&str] = &[
    "skill/SKILL.md",
    "docs/using/work-items.md",
    "docs/agents/cli.md",
];

/// Every `pinrail items <command> …` in `text`, with the options that
/// follow it on its line, up to the end of the code span or a comment.
fn named(text: &str) -> Vec<(String, Vec<String>)> {
    let mut found = Vec::new();
    for line in text.lines() {
        let mut rest = line;
        while let Some(at) = rest.find("pinrail items ") {
            rest = &rest[at + "pinrail items ".len()..];
            let end = rest.find(['`', '#', '|']).unwrap_or(rest.len());
            let mut words = rest[..end].split_whitespace();
            let Some(command) = words.next() else {
                continue;
            };
            if !command.chars().all(|c| c.is_ascii_lowercase()) {
                continue;
            }
            let options = words
                .filter(|w| w.starts_with("--"))
                .map(|w| w.split('=').next().unwrap_or(w).to_string())
                .collect();
            found.push((command.to_string(), options));
        }
    }
    found
}

#[test]
fn every_items_command_and_option_the_docs_name_exists() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut wanted: BTreeMap<String, Vec<(String, &str)>> = BTreeMap::new();
    for page in PAGES {
        let text = std::fs::read_to_string(root.join(page)).unwrap();
        let found = named(&text);
        assert!(!found.is_empty(), "{page} names no pinrail items command");
        for (command, options) in found {
            let entry = wanted.entry(command).or_default();
            entry.extend(options.into_iter().map(|o| (o, *page)));
        }
    }
    // every command the loop needs is taught somewhere
    for command in ["add", "next", "show", "edit", "comment", "done"] {
        assert!(
            wanted.contains_key(command),
            "no page names pinrail items {command}"
        );
    }
    for (command, options) in wanted {
        let out = Command::new(env!("CARGO_BIN_EXE_pinrail"))
            .args(["items", &command, "--help"])
            .output()
            .unwrap();
        let help = String::from_utf8_lossy(&out.stdout);
        assert!(
            // clap names the program as it was run: pinrail.exe on Windows
            out.status.success()
                && (help.contains(&format!("Usage: pinrail items {command}"))
                    || help.contains(&format!("Usage: pinrail.exe items {command}"))),
            "the docs name pinrail items {command}, which does not exist"
        );
        for (option, page) in options {
            let listed = help
                .lines()
                .any(|l| l.trim_start().split([' ', ',']).any(|w| w == option));
            assert!(
                listed,
                "{page} names pinrail items {command} {option}, which it does not take"
            );
        }
    }
}

#[test]
fn the_commands_and_options_are_read_from_the_text() {
    let found = named(
        "Run `pinrail items finish PIN-1 --closing-note \"x\"` and `pinrail items done 4 --comment=y`.",
    );
    assert_eq!(
        found,
        vec![
            ("finish".to_string(), vec!["--closing-note".to_string()]),
            ("done".to_string(), vec!["--comment".to_string()]),
        ]
    );
}
