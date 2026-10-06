# Settings and keys

Settings are a JSON Schema in the manifest's `settings_schema`, one level
deep. Each property is a `boolean`, a `string` (with `enum` or `oneOf` of
`const` values), an `integer` or a `number`, and needs a `default`. A
`title` gives its label. The app draws a row for each in *Settings ›
Plugins*.

```js
plugin.settings.diff;                  // the current value, from init and every change
await plugin.setSetting("diff", "split"); // the app checks it and tells every open view
```

`onSettings(settings)` says when they change.

Keys: list them in the manifest's `shortcuts`:

```json
"shortcuts": [
  { "keys": "j", "does": "Next item" },
  { "keys": "a", "does": "Accept the focused item", "group": "Verdicts" }
]
```

- `keys`: modifiers (`cmd`, `ctrl`, `alt`, `shift`) joined by `+` in any
  order, then one key named as `KeyboardEvent.code` names it, without
  `Key` or `Digit`: `j`, `1`, `enter`, `arrowdown`. `cmdorctrl` means cmd
  on macOS and ctrl on Linux.
- The app lists them in its keyboard help, and forwards them to the view
  as `keydown` when the app, not the frame, has the focus.
- The app keeps `?`, `[`, `]`, `escape` and `cmd+enter` on the review
  screen, the keys of its menus (`cmd+k`, `cmd+,`, `cmd+i`, `cmd+b`,
  `cmd+[`, `cmd+]`, `cmd+shift+h`, `cmd+shift+p`, `cmd+shift+m`,
  `cmd+shift+l`, `cmd+w`, `cmd+m`, `cmd+q`), and the editing keys (`cmd+z`,
  `cmd+shift+z`, `cmd+x`, `cmd+c`, `cmd+v`, `cmd+a`). On Linux, ctrl takes
  the place of cmd. Every other declared key is forwarded.
