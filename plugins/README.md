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
| `collect` | the human pressed ⌘/Ctrl+Enter while focus was in the shell; submit if you can. Handle the same shortcut inside your own document too: once the human has clicked in the frame, the shell never sees it |
| `appearance` | `theme: "dark" \| "light"` – the shell's theme, sent before `init` and again on every change. The SDK applies it as `data-theme` on your root element; write the CSS and you are done |

Plugin → shell:

| type | fields |
|---|---|
| `ready` | – |
| `resize` | `height` in px; the shell sizes the iframe, the page scrolls. `"fill"` instead asks for a viewport-height frame that scrolls inside, for workbench-style views such as `review` |
| `draft` | `data`; the shell keeps it in sessionStorage and hands it back in `init` |
| `submit` | `data`; validated against `decision_schema` server-side |

The decision schema is the whole contract. What the fields mean is between
the plugin and the workflow that reads the decision. `hello/index.html` is
the smallest complete client; `server/priv/plugins/list/index.html` is a
full one with drafts, read-only rendering and a previous-round overlay;
`review/index.html` is a workbench-style one that fills the viewport.
