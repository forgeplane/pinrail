---
title: Plugins
summary: Find the plugin that fits, and what is installed.
menu: [building]
---
# Plugins

A plugin is one kind of review: what you send, what the person sees, and
the shape of the answer. Pick one by its "use when", never by its name.

```sh
pinrail plugins                      # every plugin, a line each: when to use it, where from, whether it works
pinrail plugins describe <plugin>    # its payload schema, an example, its decision
```

Build the payload from the schema; start from the example. `--dry-run`
checks it before anyone sees it.

Install, update or remove plugins only when the person asks you to:
`pinrail plugins install <folder or GitHub URL>`, `pinrail plugins update`,
`pinrail plugins remove <name>`. If none fits what you need to ask,
`list` (items to accept or reject) and `feedback` (questions to answer)
are always there.
