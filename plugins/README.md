# Plugins

A gate type is a directory: a manifest, two JSON Schema documents, and a
self-contained HTML bundle that wicket renders in a sandboxed iframe. The
built-in `list` type ships with the server under `server/priv/plugins/list`;
this folder holds the official plugins and a sample:

| Plugin | What it gates |
|---|---|
| [`review/`](review/README.md) | a code review: the diff, the agent's proposed comments, the human's verdicts and own comments |
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

## Looking like the rest of wicket

The app serves a stylesheet next to the SDK. Link it and your view gets the
app's tokens in both themes, the base typography and scrollbars, and a small
vocabulary of classes:

```html
<link rel="stylesheet" href="/sdk/v1/wicket-plugin.css">
```

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
