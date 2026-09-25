# __TITLE__: a Pinrail plugin

When an agent runs `pinrail submit __NAME__`, Pinrail shows this plugin's view
to a person, who decides; the decision goes back to the agent. A plugin is
this folder: what the agent sends, what the person answers, and the view
between them.

## Files

- `manifest.json`: `name`, `version`, `title`, `description`, `use_when` (the
  moment an agent should ask with this plugin; agents choose by it), the two
  schemas, `example`, `sample` and `entry`, the view's HTML.
- `schemas/payload.schema.json`: what an agent sends, JSON Schema 2020-12.
  The view can load nothing from outside the folder (no fetch, no CDN), so
  the payload carries everything it shows.
- `schemas/decision.schema.json`: what the view hands back. The app checks
  it against this schema before the agent sees it.
- `example.json`: the smallest payload that passes; agents read it in
  `pinrail plugins describe __NAME__`.
- `sample.json`: a whole review, `title` and `payload`, that people send to
  see the plugin: `pinrail submit __NAME__ --sample`.
- The view: `view/index.html` and `view/view.js`, or `src/`, built into
  `view/`, for the framework templates.

Keep the payload schema, the example, the sample and the view in step: a
change to the payload's shape touches all four.

## The view

It runs in a sandboxed frame and talks to the app through the SDK,
`Pinrail`, loaded from `/sdk/v1/pinrail-plugin.js` (its types are in
`pinrail-plugin.d.ts`, or the package's `types` export):

```js
const plugin = Pinrail.connect({
  onInit({ gate, readonly, draft }) {}, // gate.payload; draft is what the person had entered
  onCollect() { plugin.submit(decision) }, // the app's hand-over button, or ⌘/Ctrl+Enter
  onViolations(errors) {},              // the decision failed its schema: [{ path, message }]
  onSubmitted() {},                     // accepted; show it read-only
});
plugin.draft(value);                    // keeps what the person entered across reloads
plugin.status({ label: "Hand over: yes" }); // the hand-over button's words
```

- `plugin.readonly` is true for a decided review: show `gate.decision.data`,
  and don't offer to edit it.
- `Pinrail.layout()` gives the app's header-and-body skeleton;
  `/sdk/v1/pinrail-plugin.css` gives its colours, type and classes, in both
  themes. `Pinrail.markdown`, `Pinrail.escape` and `Pinrail.icon(name)` help
  render.
- Files beside the payload: declare `attachments` in the manifest; the
  payload names each as `{ "$attachment": "<name>" }`, and
  `plugin.attachmentUrl(name)` gives the view a URL for it.

## Trying it

```sh
pinrail plugins install . --link        # the app serves this folder live
pinrail submit __NAME__ --sample        # a review, and where to see it
pinrail plugins reload                  # after changing the manifest or a schema
pinrail plugins check .                 # what the app would refuse, and why
```

`submit` prints two addresses: `pinrail://reviews/<id>`, the review in the
app, and `http://127.0.0.1:<port>/preview/reviews/<id>`, the same review in a
browser. Open the preview with a browser tool to see the view as a person
does. Handing it over there checks the decision against the decision schema
and decides nothing: a decision that passes is shown as the agent would get
it, one that fails comes back to the view as violations.

A change to the view shows the next time the preview or the review is
opened; with a framework template, `npm run watch` rebuilds `view/` as you
edit `src/`. Before you hand the plugin back, run `pinrail plugins check .`,
send the sample, and check its hand-over once in the preview.

## Finding out more

- Every SDK call, with its arguments: `pinrail-plugin.d.ts` beside the
  manifest, or the package's `types.d.ts` with a framework template.
- The design language, the stylesheet's tokens and classes, themes and
  icons: `pinrail plugins guide design`.
- Everything else about building a plugin, a topic at a time:
  `pinrail plugins guide`.
