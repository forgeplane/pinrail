# Settings and keys

## Settings

A plugin's settings are a JSON Schema in the manifest's `settings_schema`,
one level deep. Each property is a `boolean`, a `string` (with `enum`, or
`oneOf` with `const` values), an `integer` or a `number`, and needs a
`default`. Its `title` is the label. The app draws a row for each setting
in *Settings › Plugins*.

```js
plugin.settings.diff;                     // the current value, from init and every change
await plugin.setSetting("diff", "split"); // the app checks it and tells every open view
```

`onSettings(settings)` says when the settings change.

## Keys

List the view's keyboard shortcuts in the manifest's `shortcuts`:

```json
"shortcuts": [
  { "keys": "j", "does": "Next item" },
  { "keys": "a", "does": "Accept the focused item", "group": "Verdicts" }
]
```

- `keys` is any modifiers (`cmd`, `ctrl`, `alt`, `shift`) joined by `+`,
  then one key, named as `KeyboardEvent.code` names it but without `Key`
  or `Digit`: `j`, `1`, `enter`, `arrowdown`. `cmdorctrl` is cmd on macOS
  and ctrl on Linux.
- The app lists the shortcuts in its keyboard help. When the app, not the
  frame, has the focus, it forwards each one to the view as a `keydown`.
- The app keeps some keys for itself: `?`, `[`, `]`, `escape` and
  `cmd+enter` on the review screen, the keys of its menus (`cmd+k`,
  `cmd+,`, `cmd+i`, `cmd+b`, `cmd+[`, `cmd+]`, `cmd+shift+h`,
  `cmd+shift+p`, `cmd+shift+m`, `cmd+shift+l`, `cmd+w`, `cmd+m`,
  `cmd+q`), and the editing keys (`cmd+z`, `cmd+shift+z`, `cmd+x`,
  `cmd+c`, `cmd+v`, `cmd+a`). On Linux, ctrl takes the place of cmd. The
  app forwards every other declared key.
