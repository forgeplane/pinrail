# Plugins

A gate type is a directory: a manifest, two JSON Schema documents, and a
self-contained HTML bundle that wicket renders in a sandboxed iframe. The
built-in `list` type ships with the server under `server/priv/plugins/list`;
this folder holds the official plugins and a sample:

| Plugin | What it gates |
|---|---|
| [`review/`](review/README.md) | a code review: the diff, the agent's proposed comments, the human's verdicts and own comments |
| [`email/`](email/README.md) | emails an agent wants to send: edit them with the changes showing, comment on a passage, send, revise or discard |
| [`artifact/`](artifact/README.md) | an HTML page an agent designed: pick elements the way DevTools does, comment on them, and the agent gets selectors back |
| [`hello/`](hello/README.md) | the smallest complete plugin, to copy from |

Register a directory of plugins (each immediate subdirectory is one plugin):

```sh
wicket types add ./plugins        # or via the API: POST /api/types/dirs {"dir": "..."}
```

## Layout

```
hello/
  manifest.json
  index.html              # inline CSS and JS, or relative assets next to it
```

`manifest.json`:

```json
{
  "name": "hello",
  "version": 1,
  "title": "Hello",
  "payload_schema": { "$ref": "payload.schema.json" },
  "decision_schema": { "type": "object", "required": ["ok"], "properties": { "ok": { "type": "boolean" } } },
  "entry": "index.html",
  "min_height": 200,
  "dev": false
}
```

- `name` is `[a-z][a-z0-9_]*`, unique across all registered directories.
- Schemas are JSON Schema 2020-12, inline or by relative `$ref` to files in
  the plugin directory. A `$ref` cannot leave the directory.
- Bump `version` when a schema or the view changes. The first gate created
  under a version snapshots the whole directory into wicket's data dir, and
  gates keep rendering and validating from that snapshot afterwards.
- `"dev": true` serves the directory live and never snapshots it; use it while
  iterating on a view.
- `settings_schema` declares settings of the plugin's own, shown as rows under
  the plugin in *Settings › Plugins*. See below.
- `shortcuts` declares the keys your view answers, so the app lists them and
  hands them over whether or not the frame has focus. See below.
- `build` names the command that produces the bundle, for a plugin written
  with a framework: `"build": { "command": "npm ci && npm run build" }`.
  Installing runs it, in a copy, and keeps what it produced. See
  *Installing* below.

## Installing

The app installs one plugin at a time, from its folder:

```sh
wicket plugins install ./plugins/review          # a copy, in the app's store
wicket plugins install ./plugins/review --link   # served live from the folder, while you work on it
```

A copy lands in the app's store under the plugin's name and major
version, hashed and recorded with where it came from; a review renders
from it from then on. Versions are semantic (`"version": "1.2.0"`; a bare
integer reads as `N.0.0`): installing an equal or higher version replaces
the line in place, an older one is refused unless `--force`, and a new
major is a new line beside the old, which stays while a review still
renders from it.

A plugin that is built — React, Svelte, anything that compiles — declares
the command in its manifest, and never relies on the app guessing:

```json
"build": { "command": "npm ci && npm run build" }
```

Installing copies the folder without `node_modules` and `.git`, runs the
command there through the shell, shows its output as it comes, and keeps
the log; a non-zero exit stops the install with the log's tail. Whatever
the command needs (`node`, `pnpm`, `deno`) must be on the `PATH`. Only the
bundle enters the store: `src/`, `tests/`, `fixtures/`, `node_modules/`,
dot-entries and the package and tool config files stay behind. A manifest
without `build` whose `entry` is missing is refused with that said.

## Settings of your own

A plugin with knobs — a diff shown inline or side by side, how files are
ordered — declares them in the manifest, and the app draws a row for each
under the plugin in *Settings › Plugins*:

```json
"settings_schema": {
  "type": "object",
  "properties": {
    "diff": { "type": "string", "title": "Diff", "description": "How a file's changes are laid out",
              "oneOf": [{ "const": "inline", "title": "Inline" }, { "const": "split", "title": "Side by side" }],
              "default": "inline" },
    "wrap": { "type": "boolean", "title": "Wrap long lines", "default": true },
    "context": { "type": "integer", "title": "Context lines", "minimum": 0, "maximum": 20, "default": 3 }
  }
}
```

It is a JSON Schema, inline or by relative `$ref` like the other two, one
level deep: every property is a `boolean`, a `string`, an `integer` or a
`number`, and every property has a `default`. `title` is the row's label,
`description` the line under it, and property order is row order. A string
with an `enum`, or a `oneOf` of `const` values with titles, becomes a choice;
a number with `minimum` and `maximum` keeps to them. A schema that breaks
these rules does not break the plugin: it loads without settings and the
row in Settings says why.

The values live in the app's `settings.json` under `plugins.<name>`, and
`PATCH /api/v1/settings` with `{"plugins": {"review": {"diff": "split"}}}`
changes one; the app checks the change against your schema. The plugin's
row in `GET /api/v1/plugins` carries the schema and the values as they
stand.

The view gets them in `init` as `settings`, every key with its value, and
again as a `settings` message whenever they change; `plugin.settings` holds
them and `onSettings` fires on a change. A control in the view writes one
back with `plugin.setSetting("diff", "split")`: the app checks it against
the schema, keeps it, and every open view of the plugin hears the new values
— the toolbar in the view and the row in Settings are one control. A value
the schema refuses comes back as `violations`.

## Keys of your own

A view that answers keys says so in the manifest, and two things follow:
the app's keyboard-shortcuts dialog (`?`) lists them under your plugin
while one of its reviews is open, and the app forwards them to your view
when the person presses them with the app rather than the frame in focus —
after clicking the top bar, say, or arriving from the inbox.

```json
"shortcuts": [
  { "keys": "j", "does": "Next proposal" },
  { "keys": "a", "does": "Accept the focused proposal", "group": "Verdicts" },
  { "keys": "cmd+shift+f", "does": "Fold every file" }
]
```

`keys` is modifiers (`cmd`, `ctrl`, `alt`, `shift`) joined by `+` and one
key, named as `KeyboardEvent.code` spells it with `Key`/`Digit` dropped:
`j`, `1`, `/`, `enter`, `escape`, `arrowdown`. A bare key is fine. `does`
is the one-line label; `group` puts entries under a caption. Order is
display order. A key the app already uses on the review screen — `?`, `t`,
`[`, `]`, `esc`, ⌘⇧M, ⌘⏎ and the ⌘ keys of its menu — stays the app's: the
dialog says so next to your entry, and the key is not forwarded. The
app's dialog lists your keys, so a view needs no help overlay of its own.

A forwarded key arrives as a `keydown` on your document, exactly as a press
inside the frame would, so the listener you already have handles both.
Only declared keys are forwarded; a view that declares none gets none.

## Looking like the rest of wicket

The app serves a stylesheet next to the SDK. Link it and your view gets the
app's tokens in both themes, the base typography and scrollbars, and a small
vocabulary of classes:

```html
<link rel="stylesheet" href="/sdk/v1/wicket-plugin.css">
```

It also gives you icons. The app serves the [Lucide](https://lucide.dev) set a
file at a time at `/sdk/v1/icons/<name>.svg`, and `Wicket.icon("check")` writes
the markup for one. Any name in the set works and you download only the ones
you name, so the set costs your view nothing. Icons take `currentColor`, so
they follow the theme along with the text around them.

`Wicket.layout()` builds that skeleton for you, and hands back the elements
rather than markup, so the header and its controls keep their listeners while
you rewrite the body on every change:

```js
const view = Wicket.layout({ title: "5 items", controls: [acceptAll, clear] });
view.content.innerHTML = rows;          // render into this
view.title("4 items").meta(["acme-api", "7 days"]);
```

Ask for a header by passing `title`, `meta`, `controls`, or `header: true`;
without any of them you get a body and nothing else. Pass `into` to build it
somewhere other than `<body>`. Writing the markup yourself with the classes
below works just as well, which is what a view with a header of its own
should do.

The body scrolls, not the document, so a `.plugin-subhead` inside it pins
directly under the header without having to know how tall the header is. The
frame still shrinks to a short view: the SDK measures the header and the body
rather than the document.

| Class | For |
|---|---|
| `.plugin-header`, `.plugin-title`, `.plugin-meta`, `.plugin-controls` | a bar that stays at the top of the frame |
| `.plugin-scroll`, `.plugin-content` | the body that scrolls, and the padded area inside it |
| `.plugin-subhead` | a heading in the body that pins under the header while its section is on screen |
| `.plugin-footer` | a bar that stays at the bottom of the body |
| `.item` with `.head`, `.id`, `.title`, `.body`, `.controls` | one thing the human says yes or no to |
| `.sev` with `.sev-blocker`, `.sev-major`, `.sev-minor`, `.sev-nit` | severity, in the app's four levels |
| `.btn` with `.primary`, `.ghost`, `.danger`, and `aria-pressed` | buttons |
| `.field`, `.note` | inputs and textareas |
| `.meta`, `.eyebrow`, `.dim`, `.faint`, `code.inl`, `kbd` | small text and chips |
| `.notice` with `.ok`, `.warn`, `.danger`, plus `.errors`, `.empty` | something to tell the human |

**These are defaults, not rules.** Your own `<style>` comes after the
stylesheet, so anything you write wins, and a view that needs a shape this
does not have should write it.

What you get by starting here is that the palette follows the app. When its
colours change your view changes with them, in both themes, and your bundle
carries no copy of them to keep in step.

`v1` in the path is the protocol major and only ever receives corrections: a
gate decided months ago still loads it, and it must render then as it did on
the day it was decided.

## The hand-over belongs to the shell

A view does not render its own submit button. The shell puts one control next
to the note box, in the same place for every gate, and pressing it (or
⌘/Ctrl+Enter anywhere) sends `collect`. Your view decides what that means: it
may submit at once, or show what would go back and submit on the next
`collect`. Say what the button should read with `status`, and the shell keeps
it disabled while a decision is in flight or the socket is down.

This is why a decision that is only a choice, such as yes or no, is held in
the view as state rather than as two submitting buttons: the human picks, then
hands over, and nothing leaves on a single click.

One field of the decision the shell does read: a top-level `verdict` of
`"approve"` or `"revise"`. A decided review with one shows as *approved* or
*changes requested* in the history, the search and the review's bar, instead
of a bare *decided*. Any other short string is shown as it is; without the
field the shell says *decided*. The `artifact` plugin uses it.

## Sandbox

The bundle loads in `<iframe sandbox="allow-scripts">` with a Content
Security Policy of `default-src 'none'` and `connect-src 'none'`. Scripts,
styles, images and fonts must be inline or files inside the plugin
directory. No fetch, no web fonts, no CDN. Everything the view needs must be
in the payload.

## The SDK

The app serves the plugin side of the protocol at `/sdk/v1/wicket-plugin.js`.
Load it and let it do the handshake; the view only renders:

```html
<script src="/sdk/v1/wicket-plugin.js"></script>
<script>
  const plugin = Wicket.connect({
    resize: "auto",                                  // or "fill" for a viewport-height frame
    onInit({ gate, previous, readonly, draft }) { render(); },
    onViolations(errors) { showErrors(errors); },
    onSubmitted(decision) { render(); },             // read-only from here on
    onCollect() { submit(); },                       // ⌘/Ctrl+Enter
  });
  plugin.submit(data);
  plugin.draft(data);                                // debounced; { flush: true } posts now
</script>
```

`v1` only ever receives fixes. See [`wicket_sdk/`](../wicket_sdk/README.md)
for the API, the helpers (`escape`, `markdown`, `previousVerdict`), and the
test harness. The protocol below is what the SDK implements; a plugin can
speak it directly instead.

## Testing a plugin

Ship `fixtures/*.json` (a partial gate: `title`, `payload`, optionally a
`decision`) and `tests/*.spec.ts` that mount the view alone under the SDK's
fake shell; `mise run test:plugins` runs them for every plugin in this folder
and for the built-in `list`. See any shipped plugin for the pattern.

While building one, `mise run dev:plugin <directory>` (or `node
wicket_sdk/testing/serve.mjs <directory> --open`) opens the view in a
browser under a shell of its own: pick a fixture, collect a decision, read
what the view posts, and see every file change reloaded. No app needed.

## Protocol

All messages are `{ "wicket": 1, "type": "...", ...fields }` over
`postMessage`. The plugin posts `ready` once its listener is installed; the
shell answers with `init`. The shell only trusts messages whose source is the
iframe; the plugin should remember `shell_origin` from `init` and ignore
other origins.

Shell → plugin:

| type | fields |
|---|---|
| `init` | `gate` (the full envelope, payload included), `previous` (the superseded gate's envelope or null), `readonly`, `draft` (what the plugin last posted as a draft, or null), `shell_origin` |
| `violations` | `errors: [{path, message}]`, JSON pointers into the rejected decision |
| `submitted` | `decision` – the decision was accepted; render read-only |
| `collect` | the human asked to hand the gate over, with the shell's button or ⌘/Ctrl+Enter. Assemble the decision and submit it, or show a confirmation first and submit on the next `collect` |
| `appearance` | `theme: "dark" \| "light"` – the shell's theme, sent before `init` and again on every change. The SDK applies it as `data-theme` on your root element; write the CSS and you are done |
| `key` | `key`, `code`, `metaKey`, `ctrlKey`, `altKey`, `shiftKey` – one of the manifest's `shortcuts`, pressed while the app rather than your frame had focus. The SDK dispatches it as a `keydown` on your document (with `wicketForwarded: true` on the event), so a view that listens for its keys needs no change; `onKey` is there as well |

`readonly` is true whenever the gate is not pending, and `gate.status` says
why: `decided`, `withdrawn` (the requester took it back), `discarded` (the
person said no and told the agent to stop; `discarded_by` and
`discarded_reason` on the envelope say who and why) or `expired`. A view
renders the same way for all four: what was there, nothing to submit.

Your frame's URL also ends in `#wicket-theme=dark` or `#wicket-theme=light`.
A message cannot reach your view before it paints, so this is how the first
theme gets there in time; the SDK reads it as it loads. Read it yourself if
you do not use the SDK.

Plugin → shell:

| type | fields |
|---|---|
| `ready` | – |
| `resize` | `height` in px; the shell sizes the iframe, the page scrolls. `"fill"` instead asks for a viewport-height frame that scrolls inside, for workbench-style views such as `review` |
| `draft` | `data`; the shell keeps it in sessionStorage and hands it back in `init` |
| `submit` | `data`; validated against `decision_schema` server-side |
| `status` | `label`; what the shell's hand-over button should read right now, e.g. "Hand over anyway" once you have warned about something |

The decision schema is the whole contract. What the fields mean is between
the plugin and the workflow that reads the decision. `hello/index.html` is
the smallest complete client; `server/priv/plugins/list/index.html` is a
full one with drafts, read-only rendering and a previous-round overlay;
`review/index.html` is a workbench-style one that fills the viewport.
