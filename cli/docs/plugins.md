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

If none returns the decision you need, build one for the task:
`pinrail docs plugins/building`. Installing someone else's plugin,
updating or removing plugins is the person's call: do it only when they
ask (`pinrail plugins install <folder or GitHub URL>`,
`pinrail plugins update`, `pinrail plugins remove <name>`). A plugin
that runs a build is refused unless the build is confirmed: show the
person the build command the refusal names, and add `--yes` only once
they approve it.
