//! The CLI reference page of the docs, rendered from the clap definition, so
//! the page says what the binary does. Built only with `--features docs`
//! and reached through `pinrail --markdown-help`; the installed CLI carries
//! none of it.

use std::collections::BTreeMap;
use std::fmt::Write;

use clap::{Arg, Command};

use crate::EXITS;
use crate::groups::GROUPS;

/// The variables the CLI reads outside clap, beside the ones flags declare.
const ENVIRONMENT: &[(&str, &str)] = &[
    (
        "PINRAIL_DATA_DIR",
        "Where the running server's `server.json` is, to find it; also where a server the CLI starts keeps its data.",
    ),
    (
        "PINRAIL_PORT",
        "The port to try when no server advertises itself. Defaults to 4747.",
    ),
    (
        "PINRAIL_SERVER_CMD",
        "How to start a server when none is running, run through `sh -c`, or `cmd /C` on Windows. `submit`, `serve`, `plugins`, `plugins describe`, `plugins check` and `plugins new --link` use it.",
    ),
    (
        "PINRAIL_TIMEOUT",
        "How many seconds `wait` and `submit --wait` wait before they give up with exit 4, when `--timeout` is not given. Unset or 0 waits without a limit.",
    ),
];

/// What the page says under one group's heading.
struct GroupNote {
    /// The group's name, as [`crate::groups`] has it.
    title: &'static str,
    /// What its commands are for and who runs them.
    about: &'static str,
    /// The pages of the docs that explain them: a label and a path.
    see: &'static [(&'static str, &'static str)],
}

/// What the page says under each group's heading. The groups themselves
/// are the CLI's, in [`crate::groups`].
const GROUP_NOTES: &[GroupNote] = &[
    GroupNote {
        title: "Sessions and questions",
        about: "What an agent sends from its session: questions that wait for the person's answers, and statuses and notes on its work. An agent runs `ask` and `status`, and the hooks that **Report status** installs run `hook`.",
        see: &[
            ("Sessions", "/docs/concepts/sessions/"),
            ("Asking questions", "/docs/agents/cli/#asking-questions"),
            ("Reporting progress", "/docs/agents/cli/#reporting-progress"),
        ],
    },
    GroupNote {
        title: "Reviews",
        about: "Submitting a review, waiting for the person's decision and reading it, with the review's rounds, events and files. An agent or a script runs these. `decide` records a decision from a test or a tool.",
        see: &[
            ("Reviews", "/docs/concepts/reviews/"),
            (
                "Submitting a review",
                "/docs/agents/cli/#submitting-a-review",
            ),
        ],
    },
    GroupNote {
        title: "Work items",
        about: "The tasks that the person and their agents share, with sub-items and dependencies. An agent runs these to find, add, take and close work items.",
        see: &[
            ("Work items", "/docs/concepts/work-items/"),
            (
                "Work items in the CLI guide",
                "/docs/agents/cli/#work-items",
            ),
        ],
    },
    GroupNote {
        title: "Agents in Pinrail",
        about: "Starting, following and stopping the agents that Pinrail runs in its terminal host. These commands act as the person, so they work only while the setting `runtime.cli_control` is on.",
        see: &[
            ("The terminal host", "/docs/concepts/terminal-host/"),
            ("Running agents", "/docs/agents/cli/#running-agents"),
            (
                "Who can act as you",
                "/docs/concepts/trust/#who-can-act-as-you",
            ),
        ],
    },
    GroupNote {
        title: "Plugins",
        about: "The plugins, which are the kinds of review. An agent runs `plugins` and `plugins describe` to learn what to ask, and the person or a plugin's author runs the commands that install, check and create plugins.",
        see: &[
            ("Plugins", "/docs/concepts/plugins/"),
            (
                "Learning what to ask",
                "/docs/agents/cli/#learning-what-to-ask",
            ),
            ("Writing a plugin", "/docs/building/writing/"),
        ],
    },
    GroupNote {
        title: "The app",
        about: "The guides that Pinrail prints for an agent, opening a review in the app, and starting the app's server when it is not running. An agent, a script or the person runs these.",
        see: &[("Finding the app", "/docs/agents/cli/#finding-the-app")],
    },
];

/// The whole page: front matter, the groups of commands, global flags,
/// every command under its group, exit codes and the environment.
pub fn render(root: &Command) -> String {
    let mut page = String::new();
    let name = root.get_name();
    let _ = writeln!(
        page,
        "---\ntitle: CLI reference\ndescription: \"Every {name} command and flag, generated from the CLI itself.\"\n---\n"
    );
    let _ = writeln!(
        page,
        "<!-- Generated by `{name} --markdown-help` from the CLI's definition. Do not edit: change the CLI's help text instead. -->\n"
    );
    let _ = writeln!(
        page,
        "Every command and flag of `{name}`, from the same definition `{name} --help` prints. For the commands in use, see [The CLI](/docs/agents/cli/).\n"
    );

    page.push_str("The commands are grouped by what they are for:\n\n");
    for group in GROUPS {
        let names: Vec<String> = group.commands.iter().map(|c| format!("`{c}`")).collect();
        let _ = writeln!(
            page,
            "- [{}](#{}): {}",
            group.title,
            anchor(group.title),
            names.join(", ")
        );
    }
    page.push('\n');

    // hidden from each command's help, so listed here from the root
    let globals: Vec<&Arg> = root.get_arguments().filter(|a| a.is_global_set()).collect();
    if !globals.is_empty() {
        page.push_str("## Global flags\n\nEvery command accepts these.\n\n");
        table(&mut page, &globals);
    }

    for group in GROUPS {
        let _ = writeln!(page, "## {}\n", group.title);
        if let Some(note) = GROUP_NOTES.iter().find(|n| n.title == group.title) {
            let _ = writeln!(page, "{}\n", note.about);
            let see: Vec<String> = note
                .see
                .iter()
                .map(|(label, path)| format!("[{label}]({path})"))
                .collect();
            if !see.is_empty() {
                let _ = writeln!(page, "See {}.\n", list(&see));
            }
        }
        // a group names its commands in alphabetical order
        for sub in group
            .commands
            .iter()
            .filter_map(|c| root.find_subcommand(c))
        {
            command(&mut page, sub, &[name]);
        }
    }

    page.push_str("## Exit codes\n\n| Code | Meaning |\n|---|---|\n");
    for (code, meaning) in EXITS {
        let _ = writeln!(page, "| `{code}` | {meaning} |");
    }

    page.push_str("\n## Environment\n\n| Variable | Used for |\n|---|---|\n");
    for (var, what) in environment(root) {
        let _ = writeln!(page, "| `{var}` | {} |", cell(&what));
    }
    page
}

/// A heading's anchor, as the docs site makes it: lower case, with a hyphen
/// for each space.
fn anchor(title: &str) -> String {
    title.to_lowercase().replace(' ', "-")
}

/// `a`, `a and b`, or `a, b and c`.
fn list(items: &[String]) -> String {
    match items.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => items.concat(),
    }
}

/// One command's section, then its own subcommands under it, in
/// alphabetical order.
fn command(page: &mut String, cmd: &Command, parents: &[&str]) {
    let path: Vec<&str> = parents.iter().copied().chain([cmd.get_name()]).collect();
    let _ = writeln!(page, "### `{}`\n", path.join(" "));
    if let Some(about) = cmd.get_long_about().or(cmd.get_about()) {
        let _ = writeln!(page, "{}\n", description(&about.to_string()));
    }
    let aliases: Vec<&str> = cmd.get_visible_aliases().chain(cmd.get_aliases()).collect();
    if !aliases.is_empty() {
        let list: Vec<String> = aliases.iter().map(|a| format!("`{a}`")).collect();
        let _ = writeln!(page, "Also: {}.\n", list.join(", "));
    }
    let _ = writeln!(page, "```sh\n{}\n```\n", usage(cmd, &path));

    let args: Vec<&Arg> = visible_args(cmd).collect();
    if !args.is_empty() {
        table(page, &args);
    }
    let mut subs: Vec<&Command> = cmd.get_subcommands().filter(|c| !c.is_hide_set()).collect();
    subs.sort_by_key(|c| c.get_name());
    for sub in subs {
        command(page, sub, &path);
    }
}

/// `pinrail submit <PLUGIN> --title <TITLE> [OPTIONS]`: what must be given,
/// then that more is possible.
fn usage(cmd: &Command, path: &[&str]) -> String {
    let mut parts: Vec<String> = path.iter().map(|p| p.to_string()).collect();
    if cmd.has_subcommands() {
        parts.push(
            if cmd.is_subcommand_required_set() {
                "<COMMAND>"
            } else {
                "[COMMAND]"
            }
            .into(),
        );
    }
    let mut optional = false;
    for arg in visible_args(cmd) {
        if arg.is_positional() {
            let name = value_name(arg);
            parts.push(if arg.is_required_set() {
                format!("<{name}>")
            } else {
                format!("[{name}]")
            });
        } else if arg.is_required_set() {
            parts.push(flag(arg));
        } else {
            optional = true;
        }
    }
    if optional {
        parts.push("[OPTIONS]".into());
    }
    parts.join(" ")
}

/// Arguments, positionals first, then flags as the definition orders them.
fn table(page: &mut String, args: &[&Arg]) {
    page.push_str("| Argument | Description |\n|---|---|\n");
    let (positional, flags): (Vec<&&Arg>, Vec<&&Arg>) =
        args.iter().partition(|a| a.is_positional());
    for arg in positional.into_iter().chain(flags) {
        let name = if arg.is_positional() {
            format!("`<{}>`", value_name(arg))
        } else {
            format!("`{}`", flag(arg))
        };
        let _ = writeln!(page, "| {} | {} |", cell(&name), cell(&describe(arg)));
    }
    page.push('\n');
}

/// The help text, then what is required, the default, the choices, the
/// variable that sets it and other spellings.
fn describe(arg: &Arg) -> String {
    let mut text = arg
        .get_long_help()
        .or(arg.get_help())
        .map(|h| sentence(&h.to_string()))
        .unwrap_or_default();
    let mut notes = Vec::new();
    if arg.is_required_set() && !arg.is_positional() {
        notes.push("Required.".to_string());
    }
    let values: Vec<String> = arg
        .get_possible_values()
        .iter()
        .filter(|v| !v.is_hide_set())
        .map(|v| format!("`{}`", v.get_name()))
        .collect();
    if !values.is_empty() && takes_value(arg) {
        notes.push(format!("One of {}.", values.join(", ")));
    }
    let defaults: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|v| v.to_string_lossy().into_owned())
        .collect();
    if !defaults.is_empty() && takes_value(arg) {
        notes.push(format!("Default: `{}`.", defaults.join(" ")));
    }
    if let Some(env) = arg.get_env() {
        notes.push(format!("Env: `{}`.", env.to_string_lossy()));
    }
    let aliases: Vec<String> = arg
        .get_all_aliases()
        .unwrap_or_default()
        .iter()
        .map(|a| format!("`--{a}`"))
        .collect();
    if !aliases.is_empty() {
        notes.push(format!("Also {}.", aliases.join(", ")));
    }
    for note in notes {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&note);
    }
    text
}

/// Every variable a flag reads, then the ones the CLI reads itself.
fn environment(root: &Command) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();
    collect_env(root, &mut vars);
    for (var, what) in ENVIRONMENT {
        vars.insert(var.to_string(), what.to_string());
    }
    vars
}

fn collect_env(cmd: &Command, vars: &mut BTreeMap<String, String>) {
    // the global flags too, hidden from each command's help but read everywhere
    let args = visible_args(cmd).chain(cmd.get_arguments().filter(|a| a.is_global_set()));
    for arg in args {
        if let Some(env) = arg.get_env() {
            let help = arg
                .get_help()
                .map(|h| sentence(&h.to_string()))
                .unwrap_or_default();
            vars.entry(env.to_string_lossy().into_owned())
                .or_insert_with(|| format!("`{}`. {help}", flag(arg)).trim_end().to_string());
        }
    }
    for sub in cmd.get_subcommands() {
        collect_env(sub, vars);
    }
}

fn visible_args(cmd: &Command) -> impl Iterator<Item = &Arg> {
    cmd.get_arguments()
        .filter(|a| !a.is_hide_set() && !matches!(a.get_id().as_str(), "help" | "version"))
}

fn flag(arg: &Arg) -> String {
    let long = arg
        .get_long()
        .map(|l| format!("--{l}"))
        .unwrap_or_else(|| format!("-{}", arg.get_short().unwrap_or('?')));
    if takes_value(arg) {
        format!("{long} <{}>", value_name(arg))
    } else {
        long
    }
}

fn takes_value(arg: &Arg) -> bool {
    arg.get_action().takes_values()
}

fn value_name(arg: &Arg) -> String {
    arg.get_value_names()
        .map(|names| {
            names
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_else(|| arg.get_id().as_str().to_uppercase())
}

/// A command's help as markdown: each paragraph a sentence, and a
/// paragraph indented four spaces, as `--help` shows an example, a code block,
/// highlighted as JSON when it is JSON.
fn description(text: &str) -> String {
    text.split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .map(|paragraph| {
            if paragraph.lines().all(|line| line.starts_with("    ")) {
                let code = paragraph
                    .lines()
                    .map(|line| &line[4..])
                    .collect::<Vec<_>>()
                    .join("\n");
                let language = if serde_json::from_str::<serde_json::Value>(&code).is_ok() {
                    "json"
                } else {
                    ""
                };
                format!("```{language}\n{code}\n```")
            } else {
                sentence(paragraph)
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Help text as one sentence: joined lines, a capital, a full stop, and
/// the flags it mentions set as code, as the rest of the page sets them.
fn sentence(text: &str) -> String {
    let joined = text
        .split_whitespace()
        .map(code_flag)
        .collect::<Vec<_>>()
        .join(" ");
    let mut chars = joined.chars();
    let mut out = match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
        None => return String::new(),
    };
    if !out.ends_with(['.', '?', '!', ':']) {
        out.push('.');
    }
    out
}

/// `--wait,` becomes `` `--wait`, ``; any other word is left as it is.
fn code_flag(word: &str) -> String {
    let flag = word.trim_end_matches([',', ';', '.', ')', ':']);
    if flag.len() > 2 && flag.starts_with("--") {
        format!("`{flag}`{}", &word[flag.len()..])
    } else {
        word.to_string()
    }
}

/// Text that sits in a table cell.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::render;
    use crate::Cli;

    #[test]
    fn the_page_has_every_command_and_flag() {
        let page = render(&Cli::command());
        for heading in [
            "### `pinrail submit`",
            "### `pinrail wait`",
            "### `pinrail plugins`",
            "### `pinrail plugins install`",
        ] {
            assert!(page.contains(heading), "{heading} is missing");
        }
        for flag in [
            "`--title <TITLE>`",
            "`--decision-out <FILE>`",
            "`--timeout <TIMEOUT>`",
            "`--url <URL>`",
        ] {
            assert!(page.contains(flag), "{flag} is missing");
        }
        assert!(page.contains("| `5` | The person discarded the review"));
        assert!(page.contains("`PINRAIL_REQUESTED_BY`"));
        assert!(page.starts_with("---\ntitle: CLI reference\n"));
        assert!(
            page.contains("in the form the API takes:\n\n```json\n{\n  \"plugin\": \"list\","),
            "the example under submit is a code block"
        );
    }

    #[test]
    fn every_group_is_described_and_no_other() {
        use super::GROUP_NOTES;
        use crate::groups::GROUPS;

        for group in GROUPS {
            assert!(
                GROUP_NOTES.iter().any(|n| n.title == group.title),
                "{} has no description",
                group.title
            );
        }
        for note in GROUP_NOTES {
            assert!(
                GROUPS.iter().any(|g| g.title == note.title),
                "{} is described and is no group",
                note.title
            );
        }
    }

    /// The place of `text` in the page.
    fn at(page: &str, text: &str) -> usize {
        page.find(text)
            .unwrap_or_else(|| panic!("{text} is missing"))
    }

    #[test]
    fn the_commands_are_listed_under_their_groups() {
        let page = render(&Cli::command());
        assert!(!page.contains("## Commands"));
        // the groups, in their order, each linked from the top of the page
        let groups = [
            ("Sessions and questions", "sessions-and-questions"),
            ("Reviews", "reviews"),
            ("Work items", "work-items"),
            ("Agents in Pinrail", "agents-in-pinrail"),
            ("Plugins", "plugins"),
            ("The app", "the-app"),
        ];
        let mut last = 0;
        for (title, anchor) in groups {
            assert!(
                page.contains(&format!("- [{title}](#{anchor}): ")),
                "{title}"
            );
            let heading = at(&page, &format!("\n## {title}\n"));
            assert!(heading > last, "{title} is out of order");
            last = heading;
        }
        assert!(last < at(&page, "\n## Exit codes\n"));
        // a command is under its group's heading, and before the next one's
        let reviews = at(&page, "\n## Reviews\n");
        let items = at(&page, "\n## Work items\n");
        for command in ["attachments", "export", "submit", "withdraw"] {
            let heading = at(&page, &format!("### `pinrail {command}`"));
            assert!(reviews < heading && heading < items, "{command}");
        }
        assert!(at(&page, "### `pinrail ask`") < reviews);
    }

    #[test]
    fn commands_and_subcommands_are_in_alphabetical_order() {
        let page = render(&Cli::command());
        // within a group
        let order = ["attach", "host", "output", "runs", "start", "stop"];
        for pair in order.windows(2) {
            assert!(
                at(&page, &format!("### `pinrail {}`", pair[0]))
                    < at(&page, &format!("### `pinrail {}`", pair[1])),
                "{} comes before {}",
                pair[0],
                pair[1]
            );
        }
        // and under a command
        let subcommands = [
            "add", "comment", "done", "edit", "list", "next", "search", "show",
        ];
        for pair in subcommands.windows(2) {
            assert!(
                at(&page, &format!("### `pinrail items {}`", pair[0]))
                    < at(&page, &format!("### `pinrail items {}`", pair[1])),
                "items {} comes before items {}",
                pair[0],
                pair[1]
            );
        }
    }
}
