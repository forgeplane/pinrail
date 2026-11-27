//! The commands, grouped by what they are for: each group's name and its
//! commands, and every command in exactly one group. The CLI reference page
//! lists the commands under these groups, and describes each group itself.

/// A group of commands.
// until `--help` lists the commands by group too, only the reference page
// and the tests read the groups
#[cfg_attr(not(any(test, feature = "docs")), allow(dead_code))]
pub struct Group {
    /// The group's name, as a heading shows it.
    pub title: &'static str,
    /// The commands, by name, in alphabetical order.
    pub commands: &'static [&'static str],
}

/// Every group, in the order the reference lists them.
#[cfg_attr(not(any(test, feature = "docs")), allow(dead_code))]
pub const GROUPS: &[Group] = &[
    Group {
        title: "Sessions and questions",
        commands: &["ask", "hook", "status"],
    },
    Group {
        title: "Reviews",
        commands: &[
            "attachments",
            "decide",
            "discard",
            "events",
            "export",
            "list",
            "rounds",
            "show",
            "submit",
            "wait",
            "withdraw",
        ],
    },
    Group {
        title: "Work items",
        commands: &["items"],
    },
    Group {
        title: "Agents in Pinrail",
        commands: &["attach", "host", "output", "runs", "start", "stop"],
    },
    Group {
        title: "Plugins",
        commands: &["plugins"],
    },
    Group {
        title: "The app",
        commands: &["docs", "open", "serve"],
    },
];

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::GROUPS;
    use crate::Cli;

    #[test]
    fn every_command_is_in_exactly_one_group() {
        let root = Cli::command();
        let grouped: Vec<&str> = GROUPS
            .iter()
            .flat_map(|g| g.commands.iter().copied())
            .collect();
        for command in root.get_subcommands().filter(|c| !c.is_hide_set()) {
            let name = command.get_name();
            let times = grouped.iter().filter(|g| **g == name).count();
            assert_eq!(times, 1, "{name} is in {times} groups");
        }
        for name in grouped {
            // the reference lists attach, which only Unix has, for every
            // platform
            if cfg!(not(unix)) && name == "attach" {
                continue;
            }
            assert!(
                root.find_subcommand(name).is_some(),
                "{name} is in a group and is no command"
            );
        }
    }

    #[test]
    fn a_group_lists_its_commands_in_alphabetical_order() {
        for group in GROUPS {
            let mut sorted = group.commands.to_vec();
            sorted.sort_unstable();
            assert_eq!(group.commands, sorted, "{}", group.title);
        }
    }
}
