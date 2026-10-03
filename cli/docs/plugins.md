---
title: Plugins
summary: Find the plugin that fits, and what is installed.
menu: [building]
---
# Plugins

A plugin is one kind of review: what you send, what the person sees, and
the shape of the answer. "Use when" gives you the candidates. A plugin
fits only if it both shows what the person needs to review and returns
the decision you need: its decision schema says what it returns.

```sh
pinrail plugins                      # every plugin, a line each: when to use it, where from, whether it works
pinrail plugins describe <plugin>    # its payload schema and an example
pinrail plugins describe <plugin> --decision-schema          # what it returns
pinrail plugins describe <plugin> --example > payload.json   # a start for yours
```

Build the payload from the schema; start from the example. `--dry-run`
checks it before anyone sees it.

A plugin's full name is `<publisher>/<name>`, such as `forgeplane/list`.
Its name alone works when only one installed plugin has it; when two do,
the command is refused with both full names, and you give one of them.

If none returns the decision you need, build one for the task:
`pinrail docs plugins/building`. Installing someone else's plugin,
upgrading it or removing it is the person's call: do it only when they
ask (`pinrail plugins install <folder or zip>`,
`pinrail plugins remove <name>`). Installing runs nothing; a plugin
with a build step is built before it is installed.
