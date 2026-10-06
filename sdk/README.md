# pinrail-sdk

The plugin SDK of [Pinrail](https://pinrail.dev), for the author of a
plugin: a dev shell to work on a view in the browser, a test harness to
test it with Playwright, and the TypeScript types of the protocol and the
manifest. A plugin is created, checked and installed with the `pinrail`
command, which comes with the app:

```sh
pinrail plugins new ticket_triage --playwright   # a plugin, with a first test
cd ticket_triage && npm install && npx playwright install chromium
npx pinrail-sdk@1 dev .                          # the view in a browser, on its sample
npm test                                         # the plugin's tests, under the harness
pinrail plugins check .                          # what the app would say of the folder
pinrail plugins install . --link                 # the plugin, in the app
```

An installed plugin loads the SDK from the app, never from `node_modules`.
You need this package only while you write a plugin.

## What the package contains

| Export or file | What it is |
|---|---|
| `pinrail-sdk` (command) | `pinrail-sdk dev`: the dev shell |
| `pinrail-sdk/testing` | The test harness: `mountPlugin`, `fixture` and `reviewFrom` |
| `pinrail-sdk/types` | The protocol, the review and the manifest as TypeScript types, `window.Pinrail` among them |
| `pinrail-sdk/host` | The app's side of the protocol, which the dev shell and the harness run |
| `pinrail-sdk/sdk/v1/pinrail-plugin.js` | The SDK a view loads, with `pinrail-plugin.css` and `tokens.css` beside it in `src/` |
| `pinrail-sdk/sdk/v1/markdown.js` | The Markdown renderer, with its parser bundled |
| `pinrail-sdk/schemas/manifest.schema.json` | The JSON Schema every plugin's manifest is checked against |
| `pinrail-sdk/schemas/attachment.schema.json` | The JSON Schema of a reference to an attached file, to copy into a payload schema |

The SDK files are the ones the app serves at `/sdk/v1/` in the same
version, so a view behaves in the dev shell and the harness as it does in
the app.

## The dev shell

```sh
npx pinrail-sdk@1 dev [dir]    # options: --port N (default 4790), --no-open
```

`dev` serves the plugin in `dir` under the app's Content Security Policy,
with the SDK beside it, in a page that plays the app's part. It needs
nothing installed in the plugin's folder, and reloads the view, with its
last draft, when a file of the plugin changes.

- The bar picks the review the view opens with, from the plugin's
  `samples/*.json` and `fixtures/*.json`. Keep in `fixtures/` the reviews
  that should not ship with the plugin, such as a decided review or an
  edge case. It can also hand a decided review over as the previous
  round, and switch between read-only and editable and between the themes.
- Under the view sits the app's composer: the note to the agent, and the
  hand-over button with the label the view gives it.
- The side panel shows the settings and keys the manifest declares, every
  message the view sends, and what the view handed over, checked against
  the decision schema. It can answer a hand-over with violations.
- **JSON** shows the review's payload, the payload schema and the decision
  schema in place of the view.
- To review a view, turn on **Select**, or press <kbd>I</kbd>, and click a
  part of it to comment on that part. **Copy comments** puts every comment
  on the clipboard as Markdown, with each part's selector, to paste to the
  agent that works on the plugin. **Clear** empties the list for the next
  round.

## The test harness

A plugin's tests are Playwright specs in `tests/`, run with
`playwright test`. `pinrail plugins new --playwright` writes a first test,
with the configuration and the development dependencies, this package
among them. A test mounts the view alone, without the app or the CLI:

```ts
import { expect, test } from "@playwright/test";
import { fixture, mountPlugin } from "pinrail-sdk/testing";

test("hands over the answer", async ({ page }) => {
  const plugin = await mountPlugin(page, pluginDir, { review: fixture("samples/ticket_triage.json") });
  await plugin.frame.getByRole("button", { name: "Yes" }).click();
  expect(await plugin.handOver()).toMatchObject({ decision: { data: { ok: true } } });
});
```

`handOver()` hands over as the app does: it asks the view for its
decision, checks it against the decision schema, and returns the accepted
decision, the violations, or `{ deferred: true }` when the view returned
nothing. [Testing a plugin](https://pinrail.dev/docs/building/testing/)
describes every call.

## The SDK in a view

```html
<link rel="stylesheet" href="/sdk/v1/pinrail-plugin.css">
<script src="/sdk/v1/pinrail-plugin.js"></script>
<script src="/sdk/v1/markdown.js"></script> <!-- only to render Markdown -->
<script>
  const plugin = Pinrail.connect({
    onInit({ review, previous, readonly, draft, settings }) { render(); },
    onCollect() { return decision(); }, // the app's hand-over button, or ⌘/Ctrl+Enter
  });
</script>
```

A view takes its types from `pinrail-plugin.d.ts`, which
`pinrail plugins new` writes into the plugin's folder. `pinrail-sdk/types`
has the same types, for a project that installs the package. The docs
describe the SDK in full:

- [Writing a plugin](https://pinrail.dev/docs/building/writing/): the
  manifest, the schemas, the view and its calls.
- [Design and styling](https://pinrail.dev/docs/building/design/): the
  stylesheet's tokens and classes, `Pinrail.layout()`, icons and themes.
- [Settings and keys](https://pinrail.dev/docs/building/settings-and-keys/):
  a plugin's own settings and keyboard shortcuts.
- [Building with a framework](https://pinrail.dev/docs/building/frameworks/):
  one plugin in React, Vue, Svelte and TypeScript.
- [Testing a plugin](https://pinrail.dev/docs/building/testing/): the
  harness in full.
- [The protocol](https://pinrail.dev/docs/building/protocol/): every
  message, for a view written without the SDK.
- [Publishing a plugin](https://pinrail.dev/docs/building/publishing/):
  releases and installation.

## Versions

The package's version is the SDK's version, and its major version is the
protocol's (`Pinrail.protocol`): the `1.x` package is the SDK the app
serves at `/sdk/v1`.

## Developing the SDK

The SDK lives in the `sdk/` folder of the
[Pinrail repository](https://github.com/forgeplane/pinrail).
[DEVELOPING.md](https://github.com/forgeplane/pinrail/blob/main/sdk/DEVELOPING.md)
describes the folder, where its files go in the app, and the package's
own tests.

## License

The package is licensed under the Apache License 2.0; see `LICENSE` and
`NOTICE`.
