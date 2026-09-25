---
title: The manifest
summary: The JSON Schema every plugin's manifest is checked against.
menu: []
---
# The manifest

`manifest.json` is checked against this schema when the plugin is
installed or checked. A broken `settings_schema`, `shortcuts`,
`decision_template`, `example` or `sample` costs the plugin that feature,
not its place; anything else refuses it. `pinrail plugins check <dir>` says
which.

```json
{{manifest_schema}}
```
