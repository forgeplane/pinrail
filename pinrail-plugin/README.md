# Pinrail plugin SDK

The `@forgeplane/pinrail-plugin` package contains everything you need to
write a Pinrail plugin:

- **The SDK**, `src/pinrail-plugin.js`, with its stylesheet and its
  Markdown renderer. The app serves them at `/sdk/v1/`, and a view loads
  them with tags in its page. The SDK handles the protocol for the view:
  the handshake, drafts, the hand-over, settings, forwarded keys and the
  theme.
- **`pinrail-plugin create`** creates a new plugin folder.
- **`pinrail-plugin dev`** runs a plugin in the browser, without the app.
- **`pinrail-plugin test`** runs a plugin's tests in the test harness.
- **`pinrail-plugin check`** reports what the app would report for a
  plugin folder, by running `pinrail plugins check`.
- **`@forgeplane/pinrail-plugin/testing`** is a Playwright harness that
  mounts a plugin on its own.
- **`@forgeplane/pinrail-plugin/types`** describes the protocol and the
  manifest in TypeScript.

You need the package only while you write a plugin. An installed plugin
loads the SDK from the app, never from `node_modules`. A plugin without a
build step does not need the package at all: `pinrail plugins new` writes
the plain template with the SDK's types beside it, and `pinrail plugins
check` and the browser preview replace `check` and `dev`.

The package is not published on npm. Install it from this repository
(`"@forgeplane/pinrail-plugin": "file:../../pinrail-plugin"`) or from the
tarball attached to its GitHub release.

## Quick start

From a checkout of this repository:

```sh
node pinrail-plugin/bin/pinrail-plugin.mjs create ticket_triage
cd ticket_triage && npm install && npx playwright install chromium
npx pinrail-plugin dev          # the view in a browser, on its sample
npm test                        # the plugin's tests, under the harness
npx pinrail-plugin check        # what the app would say of the folder
pinrail plugins install . --link
```

The new plugin asks a yes-or-no question. Change its schemas, its view and
its sample to make it your own.

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

The docs describe the SDK in full:

- [Writing a plugin](https://pinrail.dev/docs/building/writing/) covers
  the manifest, the schemas, the view and its calls, and the tests.
- [Design and styling](https://pinrail.dev/docs/building/design/) covers
  the stylesheet's tokens and classes, `Pinrail.layout()`, icons and
  themes.
- [Settings and keys](https://pinrail.dev/docs/building/settings-and-keys/)
  covers a plugin's own settings and keyboard shortcuts.
- [Building with a framework](https://pinrail.dev/docs/building/frameworks/)
  builds one plugin in React, Vue, Svelte and TypeScript.
- [The protocol](https://pinrail.dev/docs/building/protocol/) lists every
  message, for a view written without the SDK.
- [Publishing a plugin](https://pinrail.dev/docs/building/publishing/)
  covers releases and installation.

`types.d.ts` declares every call with its arguments.

## Commands

### create

```sh
node pinrail-plugin/bin/pinrail-plugin.mjs create <name>                    # view/index.html and view/view.js, no build
node pinrail-plugin/bin/pinrail-plugin.mjs create <name> --template vite    # src/ in TypeScript, built by Vite into view/
node pinrail-plugin/bin/pinrail-plugin.mjs create <name> --template react   # the view in React, built by Vite
```

The folder runs under `dev`, passes its own tests and installs with
`--link` before you change anything. It holds the manifest, the schemas,
the view, a sample in `samples/<name>.json`, a test, an `AGENTS.md` for a
coding agent, and a release workflow. `--dir` creates the folder somewhere
other than `./<name>`, and `--sdk` sets where `package.json` installs this
package from. By default, it uses the tarball attached to the SDK's GitHub
release.

### dev

```sh
npx pinrail-plugin dev .                       # in a plugin folder that has the package installed
mise run dev:plugin plugins/hello             # in this repository
                                              # options: --port N (default 4790), --no-open
```

`dev` serves the view under the app's Content Security Policy, with the
SDK beside it, in a page that plays the app's part. The page offers the
plugin's `samples/*.json` and `fixtures/*.json` files. Keep in `fixtures/`
the reviews that should not ship with the plugin, such as a decided review
or an edge case. The page can also hand a decided review over as the
previous round, switch between read-only and editable and between the
themes, and answer a hand-over with violations. A log shows every message
the view sends. A change to any file in the plugin reloads the view with its
last draft.

Under the view sits the app's composer: the note to the agent, and the
hand-over button with the label the view gives it, which asks the view for
its decision as the app's button does.

To review a view, turn on **Select** (or press `I`) and click any part of
it, or of the composer: instead of reaching it, the click opens a comment on
that element.
Comments are kept in the browser for the plugin, grouped by the review and
mode you made them in, and marked on the view with numbered pins. **Copy
comments** puts them on the clipboard as Markdown, with each element's
selector, to paste to the agent that works on the plugin. Once it has made
the changes, the agent clears them with a request to the shell:

```sh
curl -X POST http://127.0.0.1:4790/dev/clear-comments
```

**JSON** shows, in place of the view, the review's payload and the schemas the payload and the decision are held to, each on a tab of its own.

### test

`pinrail-plugin test [dir]` runs the plugin's `tests/*.spec.ts` with the
Playwright from the plugin's own dependencies. Other arguments are passed
to Playwright, such as `-g "hands over"` or `--headed`. A test mounts the
view alone, without the app or the CLI:

```ts
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";

const plugin = await mountPlugin(page, pluginDir, { review: fixture("samples/ticket_triage.json") });
await plugin.frame.getByRole("button", { name: "Yes" }).click();
expect(await plugin.handOver()).toMatchObject({ decision: { data: { ok: true } } });
```

`handOver()` hands over as the app does: it asks the view for its decision,
checks it against the decision schema, and returns the accepted decision,
the violations, or `{ deferred: true }` when the view returned nothing.

### check

`pinrail-plugin check [dir]` runs `pinrail plugins check [dir]`, which
checks a plugin folder with the app's own rules, without the app running.
A **problem** means that the app would refuse the folder, and a **warning**
that it would install the plugin without one feature. `--json` prints the
results as JSON. The command exits with 0 when the app would take the
plugin and with 2 when it would refuse it.

## Developing the package

```sh
npm test    # the unit tests, then the browser tests, including what create writes
```

The package's version is the SDK's version, and its major version is the
protocol's (`Pinrail.protocol`): the `1.x` package is the SDK the app serves
at `/sdk/v1`. The app copies `src/` into the files it serves on every build,
so the app and the package contain the same files at every commit.

## License

The package is licensed under the Apache License 2.0; see `LICENSE` and
`NOTICE`. The files that `pinrail-plugin create` writes into a new plugin come
from `templates/`, which is licensed under MIT No Attribution
(`templates/LICENSE`). You can license a plugin made from them however you
like, with no notice to keep.
