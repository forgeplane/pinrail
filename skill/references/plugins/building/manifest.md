# The manifest

The app checks `manifest.json` against this schema when the plugin is
installed or checked. An invalid `settings_schema`, `shortcuts` or
`summary` costs the plugin only that feature, and the plugin can still be
used. Any other invalid key makes the plugin unusable. The same holds for
an icon, a template or a sample that does not load. The schemas and the
view are files at fixed places, not keys. `pinrail plugins check <dir>`
reports the problems a folder has.

```json
{{manifest_schema}}
```
