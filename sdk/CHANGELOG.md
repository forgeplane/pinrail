# Changelog

The package's version is the SDK's, and its major is the protocol's:
`1.x` speaks protocol 1, served by the app at `/sdk/v1`. A breaking
change to the protocol is a new major and a new path.

## [1.4.0]

### Added

- A comment field in the stylesheet: `.pinrail-compose`, one box with the
  text and the buttons at its end, as the official plugins draw a comment
  or a note. The stylesheet draws the add button's arrow and the cancel
  button's cross. `Pinrail.fitField(textarea)` grows the text, and puts
  the keys and the buttons on a row under it once it wraps. The markup,
  the keys and what the buttons do stay the view's.
- A saved comment's edit and delete buttons: `.pinrail-comment-edit` and
  `.pinrail-comment-delete` in a `.pinrail-comment-actions` group, with
  the pencil and the bin drawn by the stylesheet. In a comment marked
  `.pinrail-comment`, they show while the pointer is over it or the focus
  is in it.
- A comment field, a `.pinrail-compose-input`, has spell checking,
  autocorrection, automatic capitals and suggestions off. A field keeps an
  attribute its view set itself, such as `spellcheck="true"`.

## [1.3.0]

### Added

- Copying to the clipboard. A plugin that declares
  `"capabilities": ["clipboard"]` in its manifest can ask the app to copy
  text, or an image the review carries, with `plugin.copy({ text })` or
  `plugin.copy({ attachment })`, and `plugin.canCopy` says whether the app
  offers it. The view sends `clipboard_write`, and the app answers with
  `clipboard`. The host takes a `clipboard` function and a `canCopy` check.
  The dev shell copies to the browser's clipboard, and the test harness
  records each copy for `copied()`.

### Fixed

- Dictated text now reaches fields that React controls, so a view's
  state follows the dictation, and a button that waits for text, such as
  Add comment, is enabled as you dictate.
- The bundled Markdown parser is markdown-it 15.0.3, which fixes two
  cases of quadratic-time parsing: inline HTML, and a link reference
  definition over several lines.

## [1.2.0]

### Added

- Dictation. The app can turn speech into text for a view: `init`
  names the `"dictation"` capability, and the view asks with
  `dictation_start` and `dictation_stop`. The app answers with
  `dictation` messages carrying the text heard so far, then that it is
  done. In the SDK, a text field marked `data-pinrail-dictate` gets a
  microphone button, after it or inside an element marked
  `data-pinrail-dictate-for` with the field's id, and
  `plugin.dictate(field)`, `plugin.stopDictating()`, `plugin.canDictate`
  and `plugin.dictating` serve a control of the view's own. The text goes in at the cursor, with an `input` event for
  each change. A field the view draws again with the same `id` keeps the
  dictation, and Esc stops it before the view hears the key. The host
  takes a `dictation` listener and reports `onDictation`. The test
  harness mounts a view with `dictation: true`, and speaks into it with
  `say(text)` and `endDictation(error)`.

- The development shell sizes the frame from a menu in its bar: the full
  stage, the app's side panel at 700 or 420 pixels, or a phone or tablet,
  such as an iPhone or an iPad. A caption under the frame gives its size,
  and the shell remembers the choice.

### Changed

- Esc goes up to the app from a view that leaves it alone, outside a
  text field, so the app can close its panel with it. A view that uses
  Esc, for example to close a dialog of its own, keeps it with
  `preventDefault`.
- J and K belong to the app, which moves through the inbox with them
  while a review is open beside it. Pressed in a view outside a text
  field, they go up to the app as `key` messages, as `?`, `[` and `]` do.
  A manifest that declares `j`, `k`, `shift+j` or `shift+k` loses its
  shortcuts, and the test harness's `sendKey` refuses them. Plugins move
  between items with `n` and `p`, and `shift+n` and `shift+p` for a wider
  step.

## [1.1.0]

### Changed

- A decision's `decided_by` can be `null`: the app names the person only
  for a decision made in the app, and nobody when a program that names
  no one made it.
- ⌘D (Ctrl+D on Linux) is one of the keys the app keeps for itself, as
  the app now discards a review with it. Pressed anywhere in a view, a
  text field included, it goes up to the app as a `key` message, as ⌘↵
  does, unless the view called `preventDefault()` on it. In the test
  harness, `sendKey` refuses it, as it refuses the app's other keys.

### Added

- The app's find bar searches a view's text. The app sends a `find`
  message, and the SDK answers it with `found`: it searches the text the
  view shows, highlights the matches with the CSS Custom Highlight API,
  and scrolls to the current one. ⌘F (Ctrl+F on Linux) pressed in a view
  goes up to the app as a `key` message, and the test harness's `sendKey`
  refuses it. The host module gains `find()` and an `onFound` callback.
- `/sdk/v1/highlight.js`, a third optional script, highlights code with
  highlight.js 11 and its common languages, plus Elixir, Erlang, Scala,
  Haskell and a few more. It adds `Pinrail.highlight(code, language)`,
  `Pinrail.highlightLines(code, language)`, which gives one string per
  line, and `Pinrail.languageOf(path)`. The stylesheet colours the tokens
  in both themes with new `--pinrail-code-*` tokens.

## [1.0.0]

The first release of the plugin SDK, `pinrail-sdk`. It
provides:

- The script and stylesheet that plugin views load from the app at
  `/sdk/v1`: the messages with the app, drafts, the hand-over, Markdown
  rendering, icons, links, attached files and forwarded shortcuts.
- `pinrail-sdk dev`, which runs a view in a browser without the app.
- `pinrail-sdk/testing`, the test harness: `mountPlugin`,
  `fixture` and `reviewFrom`.
- `pinrail-sdk/types`, the manifest, the review, the
  messages and `window.Pinrail` as TypeScript types.

[1.4.0]: https://github.com/forgeplane/pinrail/releases/tag/sdk-v1.4.0
[1.3.0]: https://github.com/forgeplane/pinrail/releases/tag/sdk-v1.3.0
[1.2.0]: https://github.com/forgeplane/pinrail/releases/tag/sdk-v1.2.0
[1.1.0]: https://github.com/forgeplane/pinrail/releases/tag/sdk-v1.1.0
[1.0.0]: https://github.com/forgeplane/pinrail/releases/tag/sdk-v1.0.0
