# Changelog

The package's version is the SDK's, and its major is the protocol's:
`1.x` speaks protocol 1, served by the app at `/sdk/v1`. A breaking
change to the protocol is a new major and a new path.

## Unreleased

- The test harness knows audio files by their extension: `.ogg`, `.oga`
  and `.opus` as `audio/ogg`, `.m4a` as `audio/mp4`, `.aac` and `.flac`,
  beside `.mp3` and `.wav`. A fixture no longer needs a `media_type` for
  them.

- `pinrail-plugin dev` loads the fixture you choose. Choosing another
  pointed the frame at the address it already showed, which a browser does
  not load again, so the view kept the first fixture. A decided fixture is
  marked as such in the menu.

- `pinrail-plugin create <name> --template react`, `vue` or `svelte`: the
  same yes-or-no plugin as the `vite` template, with its view in React, Vue
  or Svelte.

- Files beside a payload. A plugin that declares `attachments` in its
  manifest (`{"accept": [".glb", "image/*"]}`) receives files the agent
  sent with `pinrail submit --attach`; the payload names each one
  `{ "$attachment": "pivot.glb" }`. `plugin.attachments` lists them,
  `plugin.attachment(name)` resolves with the bytes and
  `plugin.attachmentUrl(name)` with a `blob:` URL for an image, a video or a
  sound. The frame still fetches nothing: the shell hands the bytes over.
  `{ round: "previous" }` reads a file of the round this one revises.
  An app too old for this rejects with a message that says so.
- `Pinrail.attachmentName(ref)` and `Pinrail.ATTACHMENT_SCHEMA`, the
  reference as JSON Schema for a payload schema's `$defs`.
- The harness and `pinrail-plugin dev` hand files over too: a fixture lists
  them as `"attachments": {"pivot.glb": {"path": "pivot.glb"}}`, beside it,
  and `mountPlugin` takes `attachments` and `capabilities`.

- `Pinrail.markdown(s)` renders CommonMark: headings, tables, blockquotes,
  nested lists and the rest. The script the app serves carries its parser
  ([markdown-it](https://github.com/markdown-it/markdown-it), MIT), so a
  view loads one file and waits for nothing; the small hand-rolled subset
  it replaces is gone. Raw HTML is still escaped, and a link to anything
  but `http`, `https` or `mailto` keeps its text and loses its address.
- `Pinrail.markdownInline(s)`: the same for one line, without a paragraph
  around it.
- A link in a view opens in the system browser. The frame is sandboxed
  and can open nothing itself, so a click on `a[href]` becomes an `open`
  message and the shell follows it; `plugin.open(url)` asks for the same
  from a view's own code. Only `http`, `https` and `mailto` are sent.
- The stylesheet styles what markdown renders — headings, tables,
  blockquotes, rules and code — so a view styles the box, not the prose.

## 1.8.0

The first release as a package, `pinrail-plugin`. The SDK the app serves
is unchanged; around it:

- `pinrail-plugin create <name>`: a plugin folder from the `plain` or the
  `vite` template, with its schemas, view, fixture, test, README and
  release workflow, starting at `0.1.0`.
- `pinrail-plugin dev [dir]`: the shell that runs a view in the browser
  without the app, now open by default and printing the app's `--link`
  line.
- `pinrail-plugin test [dir]`: the plugin's tests under the harness, with
  Playwright from the plugin's own dependencies.
- `pinrail-plugin check [dir]`: what the app's inspect would say of the
  folder, with `--json`.
- `pinrail-plugin/testing`: `mountPlugin`, `fixture`, `gateFrom`, as
  JavaScript with a declaration file.
- `pinrail-plugin/types`: the manifest, the envelope, the messages both
  ways and `window.Pinrail`, as TypeScript.
- The harness and the shell hand a view the envelope the app sends
  (`plugin`, `origin`, `revises`), not the old server's names.
- Icons in the shell and the harness come from the pinned `lucide-static`
  release, so they render outside a checkout of the app.
