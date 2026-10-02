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
# make it yours: the decision and payload schemas, the view, the samples
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
- A manifest needs `name` and `version`. Always give `use_when` too, the
  moment an agent should ask with the plugin, specific: agents choose by
  it.
- Each file has a fixed place, which the manifest does not name:
  `schemas/payload.schema.json` and `schemas/decision.schema.json`
  (required), `view/index.html` (required, with its scripts and assets in
  `view/`), `templates/decision.md.j2`, `icon.svg`, `samples/<name>.json`,
  `README.md` and `LICENSE`. An install copies these and nothing else.
  The SDK's types are in `pinrail-plugin.d.ts`.
- A sample, `samples/<name>.json`, is a whole review (`title`, `payload`
  and any `attachments`, relative to `samples/`) for people to see the
  plugin. `--sample <name>` sends one; `--sample` alone sends the first
  by name. The first sample's payload is also the example agents get
  from `pinrail plugins describe`, so keep it small. A change to the
  payload's shape touches the payload schema, the view and the samples.
- `summary` in the manifest declares what the app counts for the inbox
  and history: arrays of the payload (`request`) and of the decision
  (`outcome`), by a field such as `severity` or `action`, each value with
  a label and a tone. Without it, a review shows no summary. The format
  is in the `manifest` brief's schema.
- Read the schemas, not prose about them: your plugin's own are
  `schemas/payload.schema.json` and `schemas/decision.schema.json`, JSON
  Schema 2020-12; an installed plugin's are in
  `pinrail plugins describe <plugin>`.
- The preview's hand-over checks the decision against the decision schema
  and decides nothing. A change to the view shows the next time the
  preview is opened; after a change to the manifest or a schema,
  `pinrail plugins reload`.
- The view runs in a sandboxed frame with an opaque origin, so a
  browser tool that reads or clicks through the page's DOM or
  accessibility tree sees the frame as empty, even when the view is
  drawn. To check a view, take a screenshot, or drive the preview with
  Playwright and reach inside with `page.frameLocator("iframe")`. An
  empty frame in your tool does not mean the view is broken.
