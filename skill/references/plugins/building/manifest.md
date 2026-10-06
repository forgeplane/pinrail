# The manifest

`manifest.json` is checked against this schema when the plugin is
installed or checked. If `settings_schema`, `shortcuts` or `summary` is
invalid, the plugin loses only that feature and can still be used. Any
other invalid key means the plugin cannot be used. The same holds for an
icon, a template or a sample that does not load. The schemas and the view
are files at fixed places, not keys. `pinrail plugins check <dir>` reports
which problems a folder has.

```json
{{manifest_schema}}
```
