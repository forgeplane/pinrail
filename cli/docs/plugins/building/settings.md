---
title: Settings and keys
summary: Options in Settings › Plugins, and keyboard shortcuts the app lists and forwards.
menu: []
---
# Settings and keys

Settings: a JSON Schema in the manifest's `settings_schema`, one level
deep, every property a `boolean`, `string` (with `enum` or `oneOf` of
`const` values), `integer` or `number`, each with a `title` and a
`default`. The app draws a row for each in *Settings › Plugins*.

```js
plugin.settings.diff;                  // the current value, from init and every change
plugin.setSetting("diff", "split");    // the app checks it and tells every open view
```

`onSettings(settings)` says when they change.

Keys: list them in the manifest's `shortcuts`:

```json
"shortcuts": [
  { "keys": "j", "does": "Next item" },
  { "keys": "a", "does": "Accept the focused item", "group": "Verdicts" }
]
```

- `keys`: modifiers (`cmd`, `ctrl`, `alt`, `shift`) joined by `+`, then
  one key named as `KeyboardEvent.code` names it, without `Key` or
  `Digit`: `j`, `1`, `enter`, `arrowdown`.
- The app lists them in its keyboard help, and forwards them to the view
  as `keydown` when the app, not the frame, has the focus.
- The app keeps `?`, `[`, `]`, `escape`, `cmd+shift+m` (⌘⇧M),
  `cmd+enter` (⌘↵) and its menus' keys.
