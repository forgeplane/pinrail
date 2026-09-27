---
title: The manifest
summary: The JSON Schema every plugin's manifest is checked against.
menu: []
---
# The manifest

`manifest.json` is checked against this schema when the plugin is
installed or checked. If `settings_schema`, `shortcuts`,
`decision_template`, `example`, `sample` or `icon` is invalid, the plugin
loses only that feature and can still be used. Any other invalid key means
the plugin cannot be used. `pinrail plugins check <dir>` reports which
problems a folder has.

```json
{{manifest_schema}}
```
