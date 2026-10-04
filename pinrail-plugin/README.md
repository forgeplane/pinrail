# Pinrail plugin SDK

The `@forgeplane/pinrail-plugin` package contains everything you need to
write a Pinrail plugin:

- **The SDK**, `src/pinrail-plugin.js`, and its stylesheet. The app serves
  the SDK at `/sdk/v1/pinrail-plugin.js`, bundled with the markdown-it
  parser. A view loads it with a single script tag, and the SDK handles the
  protocol for it: the `ready` message, origin pinning, resizing, drafts,
  the hand-over, the `submitted` and `violations` messages, and the
  ⌘/Ctrl+Enter shortcut.
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

```html
<script src="/sdk/v1/pinrail-plugin.js"></script>
<script>
  const plugin = Pinrail.connect({
    resize: "auto",                     // "auto" (content height), "fill" (viewport), "manual"
    onInit({ review, previous, readonly, draft }) { render(); },
    onViolations(errors) { showErrors(errors); },   // [{ path, message }]
    onSubmitted(decision) { render(); },           // the view is now read-only
    onCollect() { return decision(); },            // the app's hand-over button, or ⌘/Ctrl+Enter
    onAppearance(theme) { … },                     // optional: "dark" | "light"
    onSettings(settings) { render(); },            // optional: the plugin's settings changed
    onError(error) { … },                          // optional: a handler threw, or a decision is not JSON
  });
  plugin.draft(data);                   // kept at once; it comes back in onInit
  plugin.handOverLabel("Hand over anyway");   // the label of the shell's button
  plugin.readonly; plugin.review; plugin.previous;
  plugin.settings;                      // the plugin's settings, with every key the manifest declares
  await plugin.setSetting("diff", "split"); // asks the app to keep a value: the settings, or why not
</script>
```

The app provides the button that hands a decision over. A view does not
render its own submit button: the app shows one next to the note field of
every review. When the person presses it, or ⌘/Ctrl+Enter, the SDK calls
`onCollect`, and the view returns the decision, or a promise of it. When the
view needs more from the person first, such as a missing answer or a
preview to confirm, it returns nothing: the app hands nothing over, and the
next press asks again. Call `status` to keep the button's label accurate.

The shell also sets the theme. The theme is part of the frame's URL when
the frame opens, and every later change arrives as an `appearance` message.
The SDK reads the URL as it loads and sets `data-theme` on the root element
immediately, so the first frame the view paints already uses the shell's
theme. The current theme is also available as `plugin.theme`. A view only
needs the CSS:

```css
:root { --bg: #18191b; --text: #ededef; color-scheme: dark; }
[data-theme="light"] { --bg: #fff; --text: #24262c; color-scheme: light; }
```

Load the SDK with a plain `<script src>` tag. A script with `defer` or
`type="module"` runs after the document has painted, which is too late to
set the colours. A theme change never initialises the view again and never
changes its draft.

## Plugin settings

When the manifest declares a `settings_schema`, the view receives the
settings in `init` as `settings`: every key the schema declares, with the
value the person set or else its default. The view receives them again as
a `settings` message whenever they change, whether in the app's Settings or
from the view itself. `plugin.settings` holds the current values, and
`onSettings` is called on every change. A change never initialises the view
again and never changes its draft. The manifest keys are described in
[Settings and keys](../docs/building/settings-and-keys.md).

A view stores a value with `plugin.setSetting(key, value)`. The app adds
the plugin's name, so a view can change only its own settings, and checks
the value against the schema. The promise `setSetting` returns resolves with
the settings when the app keeps the value, and rejects when it refuses it:
the error's `violations` say why. Every open view of the plugin hears the
new values through `onSettings`. As a result, a control in the view and the
row in Settings change the same value.

## Plugin keyboard shortcuts

When the manifest declares `shortcuts`, the view receives those keys as
`key` messages when the person presses them while the app, rather than the
view's frame, has focus. The SDK dispatches each key as a `keydown` event on
the document, marked with `pinrailForwarded`, so the view's existing key
listener handles a forwarded key like a typed one. The `shortcuts` key is
described in
[Settings and keys](../docs/building/settings-and-keys.md).

## Icons

A plugin supplies its own icons. A view without a build step keeps them as
SVG files in `view/icons/`, and `Pinrail.icon(name)` returns the markup for
`icons/<name>.svg`:

```js
`<button class="btn">${Pinrail.icon("check")} Accept</button>`
```

The app uses [Lucide](https://lucide.dev) icons, so Lucide icons fit its
style best. Copy the icons you use from the `lucide-static` package, and keep
the licence comment (ISC) at the start of each file. The icon is drawn as a
CSS mask, so it takes the colour of the surrounding text (`currentColor`) in
both themes without any configuration. Its size follows the font size, and
`{ size: 18 }` or `{ size: "1.25em" }` overrides it.

An icon is decorative by default and is hidden from screen readers. Pass
`{ label: "delete" }` when the icon is the only indication of what a control
does. A name without a matching file renders as empty space, and the name
stays on the element.

A view with a build step imports its icons from its framework's Lucide
package instead, as the templates do: `lucide-react`, `@lucide/vue`,
`@lucide/svelte`, or `lucide` for plain TypeScript
(`createElement(icon, { class: "lucide" })`). The build includes only the
icons the view imports, and the stylesheet sizes an `svg.lucide` element to
the text, as it does for `Pinrail.icon`.

The plugin's own icon is `icon.svg`, at the top of its folder. The app
shows it, drawn the same way, wherever it names the plugin.

The SDK also provides `Pinrail.escape(s)`, which escapes text for use in
HTML.

## Markdown

A view can render Markdown directly:

```js
const plugin = Pinrail.connect({
  onInit({ review }) { view.content.innerHTML = Pinrail.markdown(review.payload.notes); },
});
```

`Pinrail.markdown(s)` and `Pinrail.markdownInline(s)` are available as soon
as the SDK loads. The script that the app serves includes the
[markdown-it](https://github.com/markdown-it/markdown-it) parser, so a view
does not need to load another file.

A view's frame is sandboxed and cannot open links itself. When the person
clicks a link in rendered Markdown, the SDK sends a message to the app
instead. `plugin.open(url)` sends the same message from your own code. The
app shows the person where the link leads and opens it in their browser
when they agree, or at once when they have allowed that site for your
plugin.

The output is CommonMark rendered as HTML, with headings, tables, block
quotes, nested lists and code. Raw HTML in the source is escaped rather than
passed through, because a view's frame runs inline scripts. A link whose
scheme is not `http`, `https` or `mailto` keeps its text but loses its
address. The output has no classes, so you style the plain elements.

`v1` is the protocol's major version, and it changes only in ways that keep
existing views working. The source of the SDK is `src/pinrail-plugin.js` in
this package. The desktop app's `sdk:build` step copies it into the files the
app serves, so the repository contains a single copy.

## The stylesheet

The app serves `src/pinrail-plugin.css` beside the SDK, at
`/sdk/v1/pinrail-plugin.css`. It contains the app's colour tokens for both
themes, the base typography, scroll bars, and a small set of classes for
common elements: a header and content area, items, severity chips, buttons,
form fields and notices. A view links the stylesheet and adds only its own
styles.

The stylesheet is optional, and a view can override it, because the view's
own `<style>` element comes after it. The classes are described in
[Design](../docs/building/design.md).

`Pinrail.layout()` builds the structure the stylesheet expects and returns
its elements. A view can then replace its content on every change while the
header and its controls stay in place:

```js
const view = Pinrail.layout({ title: "5 items", controls: [button] });
view.content.innerHTML = rows;
view.title("4 items").meta(["acme-api", "7 days"]).controls([]);
```

The header appears only when you pass `title`, `meta`, `controls` or
`header: true`. The `into` option places the structure in an element other
than `<body>`.

The content area scrolls instead of the document, so a heading with the
`.plugin-subhead` class stays pinned under the header. Automatic sizing
still reports the height the view needs, because it measures the header and
the content area instead of the document.

## Tests

```sh
npm test            # the unit tests, against a simulated shell, and then
                    # test/scaffold.spec.ts, which tests what create writes
```

## Creating a plugin

`pinrail-plugin create` writes a plugin folder that already runs under
`dev`, passes its own tests and installs with `--link`, before you change
anything:

```sh
node pinrail-plugin/bin/pinrail-plugin.mjs create ticket_triage                    # view/index.html and view/view.js, no build
node pinrail-plugin/bin/pinrail-plugin.mjs create ticket_triage --template vite    # src/ in TypeScript, built by Vite into view/
node pinrail-plugin/bin/pinrail-plugin.mjs create ticket_triage --template react   # the view in React, built by Vite
node pinrail-plugin/bin/pinrail-plugin.mjs create ticket_triage --template vue     # the view in Vue, built by Vite
node pinrail-plugin/bin/pinrail-plugin.mjs create ticket_triage --template svelte  # the view in Svelte, built by Vite
```

Run these commands from a checkout of this repository. For a plugin without
a build step, `pinrail plugins new` creates the same folder without a
checkout.

The new folder contains the following files:

- `manifest.json`, at version `0.1.0`. Replace its `description` and
  `use_when`.
- `samples/<name>.json`, a complete review with a `title` and a `payload`.
  `pinrail submit <name> --sample` and the app's Settings send it, and its
  payload is the example that agents get.
- `schemas/`, with one property in each schema and a description of what to
  replace.
- `view/index.html` and `view/view.js`, a yes-or-no question with comments in
  the style of the sample plugins. They are type-checked with
  `// @ts-check` against `pinrail-plugin.d.ts`, a copy of the SDK's types
  beside the manifest. A template with a build step writes `src/`,
  `vite.config.ts` and `tsconfig.json` instead.
- `AGENTS.md`, which explains the plugin to an agent that helps build it,
  and `CLAUDE.md`, which refers to it.
- `fixtures/basic.json`, a review to show in the view.
- `tests/<name>.spec.ts`, a test in the harness, with its
  `playwright.config.ts`.
- `package.json`, which depends on this package and on Playwright.
- `.gitignore`.
- `README.md`, with the commands for the plugin.
- `.github/workflows/release.yml`, which attaches `<name>-<version>.zip` to a
  GitHub release when you push a `v<version>` tag. Anyone can then install
  the plugin with `pinrail plugins install <releases URL>`.

The `--dir` option creates the folder somewhere other than `./<name>`. The
`--sdk` option sets where `package.json` installs this package from. By
default, it uses the tarball attached to the SDK's GitHub release.

## Running a plugin in the browser

`pinrail-plugin dev` runs one plugin without the app. Give it a plugin
folder, and it serves the view under the app's Content Security Policy,
with the SDK beside it, and opens a page that acts as the app's shell.

```sh
npx pinrail-plugin dev .                       # in a plugin folder that has the package installed
mise run dev:plugin plugins/artifact          # in this repository, where no package links it at the root
                                              # options: --port N (default 4790), --no-open
```

The page lists the plugin's `fixtures/*.json` files, and you choose one to
initialise the view with. You can also:

- use a decided fixture as the previous round;
- switch between read-only and editable, and between the themes;
- press **Collect**, which asks the view for its decision as the app's
  hand-over button does;
- answer a submission with `violations` that you type, or with `submitted`.

A log beside the view shows every message the view sends, such as `ready`,
`resize`, `draft`, `status`, `submit` and `defer`. A change to any file in the plugin
reloads the view, and the last draft is passed back in the next `init`, so
`dev` works well with a build in watch mode. The app can serve the same
folder at the same time: run `pinrail plugins install <dir> --link`, which
`dev` prints when it starts.

## Testing a plugin on its own

`@forgeplane/pinrail-plugin/testing` (in `harness/`) mounts a plugin folder
in a sandboxed frame, under a simulated shell with the SDK and the app's
Content Security Policy. A test exercises the view on its own, without the
app or the CLI:

```ts
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";

const plugin = await mountPlugin(page, pluginDir, { review: fixture("fixtures/basic.json") });
await plugin.frame.getByRole("button", { name: "Yes" }).click();
expect(await plugin.handOver()).toMatchObject({ decision: { data: { ok: true } } });
```

`handOver()` hands over as the app does: it asks the view for its decision,
checks it against the decision schema, and returns the accepted decision,
the violations, or `{ deferred: true }` when the view returned nothing.
`collect()` only asks, and `nextSubmit()` returns what the view answered,
for a test that replies itself with `sendViolations` or `sendSubmitted`.

A fixture holds part of a review, in the form the SDK passes to a view as
`review`. It is usually `{ "title", "payload" }`, and it includes a
`decision` for a read-only view or a previous round.

Tests live in `<plugin>/tests/*.spec.ts`. `pinrail-plugin test [dir]` runs
them with the Playwright from the plugin's own dependencies. It uses the
plugin's `playwright.config` when there is one, and the package's otherwise.
Any other arguments are passed to Playwright, such as `-g "hands over"` or
`--headed`. In this repository, `mise run test:plugins` runs the tests of
every plugin in `plugins/`, which depend on this package by path.

## Checking a plugin

`pinrail-plugin check [dir]` runs `pinrail plugins check [dir]`, which
checks a plugin folder with the app's own rules, without the app running
and without installing anything. It needs the `pinrail` command. It
reports two kinds of results:

- A **problem** means that the app would refuse the folder. Problems concern
  the manifest, the name, the version, `view/index.html` (or the build that
  writes it), and the schemas in `schemas/`.
- A **warning** means that the app would install the plugin but drop one
  feature, and show the reason on the plugin's row. Warnings concern a
  `settings_schema`, `shortcuts` or `summary` that is not valid, an
  `icon.svg` that is not an SVG file, a template that does not compile, and
  a sample without a title, a valid payload or its files.

`--json` prints the same results as JSON. The command exits with 0 when the
app would take the plugin and with 2 when it would refuse it.

## Types

```ts
import type { Manifest, Init, Review, AppMessage, PluginMessage } from "@forgeplane/pinrail-plugin/types";
```

`types.d.ts` describes the protocol: every manifest key the app reads, the
review a view receives, the messages in both directions, and the shape of
`window.Pinrail`.

## Versions

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
