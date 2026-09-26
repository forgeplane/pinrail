---
title: Building a plugin
summary: Make a plugin: design its decision, scaffold it, check it, link it, try its sample.
menu: [manifest, view, design, settings, frameworks]
---
# Building a plugin

Design the decision first: write `schemas/decision.schema.json` around the
exact action you will take on the answer, such as approve, send back with
changes, or stop, and what an incomplete answer means. Then build the
controls that produce it. A view that shows the right material but returns
another kind of answer does not do the job.

```sh
pinrail plugins new <name> --dir <path>   # scaffold it; no build, nothing installed
pinrail plugins check <path>              # what the app would refuse, and why
pinrail plugins install <path> --link     # the app serves the folder live
pinrail submit <name> --sample            # a real review of its sample, in the person's inbox
pinrail open <id> --browser               # its preview: the view in a browser, prints the address
pinrail withdraw <id>                     # when you are done with the sample
```

- `--link` registers the folder with the app, which serves it from there
  and lists it in the person's plugins; `pinrail plugins remove <name>`
  undoes it. Linking a plugin the person's task needs is part of that
  task. `plugins new --link` scaffolds and links in one step.
- A manifest needs `name`, `version`, `payload_schema` and
  `decision_schema`. Always give `use_when` too, the moment an agent
  should ask with the plugin, specific: agents choose by it.
- The view is `view/index.html` and `view/view.js`; the SDK's types are in
  `pinrail-plugin.d.ts`. The scaffold sets `"entry": "view/index.html"`;
  without it the app looks for `index.html` at the folder's top.
- The scaffold includes `example.json`, the smallest payload that passes,
  for agents, and `sample.json`, a whole review, `title` and `payload`,
  for people to see the plugin. Both are optional, and go together with
  the payload schema and the view: a change to the payload's shape
  touches all four.
- Read the schemas, not prose about them: your plugin's own are
  `schemas/payload.schema.json` and `schemas/decision.schema.json`, JSON
  Schema 2020-12; an installed plugin's are in
  `pinrail plugins describe <plugin>`.
- The preview's hand-over checks the decision against the decision schema
  and decides nothing. A change to the view shows the next time the
  preview is opened; after a change to the manifest or a schema,
  `pinrail plugins reload`.
