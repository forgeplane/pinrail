---
title: Building a plugin
summary: Make a plugin: scaffold it, link it, send it its sample, check it.
menu: [manifest, view, design, settings, frameworks]
---
# Building a plugin

A plugin is a folder: `manifest.json`, the payload and decision schemas,
`example.json`, `sample.json`, and the view, an HTML page the app shows in
a sandboxed frame.

```sh
pinrail plugins new <name> --link    # a plugin that needs no build, served live by the app
pinrail submit <name> --sample       # send its sample; open the preview address it prints
pinrail plugins check <dir>          # what the app would refuse, and why
pinrail plugins reload               # after changing the manifest or a schema
```

- The view is `view/index.html` and `view/view.js`; the SDK's types are in
  `pinrail-plugin.d.ts`.
- Read the schemas, not prose about them: your plugin's own are
  `schemas/payload.schema.json` and `schemas/decision.schema.json`, JSON
  Schema 2020-12; an installed plugin's are in `pinrail plugins describe
  <plugin>`.
- The payload schema, `example.json`, `sample.json` and the view go
  together: a change to the payload's shape touches all four.
- `sample.json` is a whole review, `title` and `payload`, for people to
  see the plugin; `example.json` is the smallest payload that passes.
- `use_when` in the manifest is the moment an agent should ask with the
  plugin. Agents choose by it: name the moment, not the feature.
- The preview shows the view in a browser. Its hand-over checks the
  decision against the decision schema and decides nothing.
- A change to the view shows the next time the preview is opened.
