# pinrail-plugin

Everything for writing a pinrail plugin, as one npm package:

- the SDK, `src/pinrail-plugin.js` and its stylesheet: the plugin side of the
  protocol as one dependency-free file, which the app serves at
  `/sdk/v1/pinrail-plugin.js`. A plugin loads it with a single script tag and
  has the whole handshake done for it: `ready`, origin pinning, resize,
  drafts, `submitted`, `violations`, `collect` and the ⌘/Ctrl+Enter shortcut;
- `pinrail-plugin create`, a plugin folder to start from;
- `pinrail-plugin dev`, a shell that runs a plugin in the browser without the
  app;
- `pinrail-plugin test`, the plugin's tests under the harness, and
  `pinrail-plugin check`, what the app would say of the folder;
- `pinrail-plugin/testing`, a Playwright harness that mounts a plugin alone;
- `pinrail-plugin/types`, the protocol and the manifest as TypeScript.

The package is authoring-time only: a shipped plugin loads the SDK from the
app, never from `node_modules`. A plugin without a build needs none of it:
`pinrail plugins new`, in the app's own command, writes the plain template
with the SDK's types beside it, and `pinrail plugins check` and the
browser preview stand in for `check` and `dev`. The package is not on npm:
install it from this repository (`"@forgeplane/pinrail-plugin": "file:../../pinrail-plugin"`)
or from the tarball attached to its GitHub release.

```html
<script src="/sdk/v1/pinrail-plugin.js"></script>
<script>
  const plugin = Pinrail.connect({
    resize: "auto",                     // "auto" (content height), "fill" (viewport), "manual"
    onInit({ review, previous, readonly, draft }) { render(); },
    onViolations(errors) { showErrors(errors); },   // [{ path, message }]
    onSubmitted(decision) { render(); },           // now read-only
    onCollect() { submit(); },                     // the shell's hand-over button, or ⌘/Ctrl+Enter
    onAppearance(theme) { … },                     // optional: "dark" | "light"
    onSettings(settings) { render(); },            // optional: the plugin's own settings changed
    onKey(key) { … },                              // optional: a declared shortcut, pressed with the app in focus
  });
  plugin.submit(data);
  plugin.draft(data);                   // debounced 150ms; { flush: true } posts at once
  plugin.status({label: "Hand over anyway"});   // what the shell's button should read
  plugin.readonly; plugin.review; plugin.previous;
  plugin.settings;                      // the plugin's own settings, every key the manifest declares
  plugin.setSetting("diff", "split");   // asks the shell to keep one; it comes back as `settings`
</script>
```

The shell owns the hand-over. A view renders no submit button: the shell puts
one next to the note box for every review, and pressing it sends `collect`. Your
view may submit at once or confirm first and submit on the next `collect`;
`status` keeps the button's label honest.

The shell owns the theme. It is on your frame's URL when the frame opens and
arrives again as an `appearance` message on every change. The SDK reads the
URL as it loads and sets `data-theme` on your root element there and then, so
your view is in the shell's theme in the frame it first paints, never a
default first. It also exposes `plugin.theme`. A view only has to write the
CSS:

```css
:root { --bg: #18191b; --text: #ededef; color-scheme: dark; }
[data-theme="light"] { --bg: #fff; --text: #24262c; color-scheme: light; }
```

Load the SDK with a plain `<script src>` tag for this: a `defer` or `type=
"module"` script runs after the document has painted, which is too late to
choose a colour. A theme change never re-initialises the view or touches its
draft.

## Settings of the plugin's own

A manifest with a `settings_schema` (see [`plugins/README.md`](../plugins/README.md))
gets its values in `init` as `settings`, every key the schema declares with
its default under what the person set, and again as a `settings` message
whenever they change — in the app's Settings, or from the view itself.
`plugin.settings` holds them; `onSettings` fires on a change and never
re-initialises the view or touches its draft.

A view writes one with `plugin.setSetting(key, value)`. The shell fills in
the plugin's name, so a view can only ever write its own, and the app checks
the value against the schema: what it keeps comes back as `settings`, what it
refuses as `violations` with the path under `/plugins/<name>`. That is what
makes a toggle in the view and the row in Settings the same control.

## Keys of the plugin's own

A manifest with `shortcuts` (see [`plugins/README.md`](../plugins/README.md))
gets those keys as `key` messages when the person presses them with the
app rather than the frame in focus. The SDK dispatches each as a `keydown`
on the document, marked `pinrailForwarded`, so the listener a view already
has handles a forwarded key like a typed one; `onKey(key)` fires as well.

## Icons

A plugin brings its own icons. A view without a build keeps them as SVG
files in `view/icons/`, and `Pinrail.icon(name)` returns the markup for
`icons/<name>.svg`:

```js
`<button class="btn">${Pinrail.icon("check")} Accept</button>`
```

The app draws with [Lucide](https://lucide.dev), and its icons fit best:
copy the ones you use from `lucide-static`, keeping the licence comment each
file starts with (ISC). The glyph is drawn as a mask, which is what makes it
take `currentColor`: an icon is the colour of the text it sits in, in either
theme, with nothing to configure. Size follows the font size;
`{ size: 18 }` or `{ size: "1.25em" }` overrides it.

An icon is decorative by default and is not announced. Pass `{ label: "delete" }`
when the icon is the only thing saying what a control does. A name with no icon
behind it renders as empty space, with the name left on the element.

A view with a build imports its icons from its framework's Lucide package
instead, as the templates do: `lucide-react`, `@lucide/vue`, `@lucide/svelte`,
or `lucide` for plain TypeScript (`createElement(icon, { class: "lucide" })`).
The build keeps only the icons the view imports, and the stylesheet sizes an
`svg.lucide` to the text as it does `Pinrail.icon`.

The manifest's `icon` is an SVG file in the plugin's folder too, such as
`icon.svg`: the app shows it wherever it names the plugin, drawn the same way.

Helpers: `Pinrail.escape(s)` and `Pinrail.previousVerdict(previous, id)`, for
decisions shaped as `{ decisions: [{ id, action, note }], undecided: [id] }`.

## Markdown

A view renders markdown with no ceremony:

```js
const plugin = Pinrail.connect({
  onInit({ review }) { view.content.innerHTML = Pinrail.markdown(review.payload.notes); },
});
```

`Pinrail.markdown(s)` and `Pinrail.markdownInline(s)` are there from the view's
first line: the script the app serves carries its parser,
[markdown-it](https://github.com/markdown-it/markdown-it), so a view loads one
file and waits for nothing.

A view's frame is sandboxed and can open nothing itself, so a click on a link
in what it renders becomes a message to the app. `plugin.open(url)` sends the
same message from your own code. The app shows the person where the link goes
and opens it in their browser when they agree, or at once when they allowed
that site for your plugin.

What comes back is CommonMark as HTML: headings, tables, blockquotes, nested
lists, code. The HTML is the parser's own — raw HTML in the source is escaped
rather than passed through, which matters because a view's frame runs inline
scripts — and a link to anything but `http`, `https` or `mailto` keeps its text
and loses its address. Styling stays yours: plain elements, no classes.

`v1` is the protocol major: it only ever gets fixes. The source of truth is
`src/pinrail-plugin.js` here; the desktop app's `sdk:build` copies it into what
the app serves at build time, so there is exactly one copy in the repository.

## The stylesheet

`src/pinrail-plugin.css` is served beside the SDK at
`/sdk/v1/pinrail-plugin.css`. It carries the app's tokens for both themes, the
base typography and scrollbars, and a small set of classes for the furniture
every view needs: header and content, items, severity chips, buttons, fields,
notices. A view links it and writes only what is its own.

It is optional and overridable: a view's own `<style>` comes after it. The
list of classes is in [`plugins/README.md`](../plugins/README.md).

`Pinrail.layout()` builds the skeleton the stylesheet expects and returns its
elements, so a view can rewrite its body on every change while the header and
its controls stay put:

```js
const view = Pinrail.layout({ title: "5 items", controls: [button] });
view.content.innerHTML = rows;
view.title("4 items").meta(["acme-api", "7 days"]).controls([]);
```

A header appears only if you ask for one with `title`, `meta`, `controls` or
`header: true`. `into` puts the skeleton somewhere other than `<body>`.

The skeleton scrolls its body rather than the document, so a heading marked
`.plugin-subhead` pins under the header on its own. Auto sizing still reports
what the view needs, because it measures the header and the body instead of
the document.

## Tests

```sh
npm test            # the unit tests, against a fake shell environment; then
                    # test/scaffold.spec.ts, what create writes under the harness
```

## Starting a plugin

`pinrail-plugin create` writes a folder that runs under `dev`, passes its
own tests and installs with `--link` before a line of it is changed:

```sh
node pinrail-plugin/bin/pinrail-plugin.mjs create ticket_triage                    # view/index.html and view/view.js, no build
node pinrail-plugin/bin/pinrail-plugin.mjs create ticket_triage --template vite    # src/ in TypeScript, built by Vite into view/
node pinrail-plugin/bin/pinrail-plugin.mjs create ticket_triage --template react   # the view in React, built by Vite
node pinrail-plugin/bin/pinrail-plugin.mjs create ticket_triage --template vue     # the view in Vue, built by Vite
node pinrail-plugin/bin/pinrail-plugin.mjs create ticket_triage --template svelte  # the view in Svelte, built by Vite
```

Run these commands from a checkout of this repository. For a plugin
without a build step, `pinrail plugins new` creates the same folder
without a checkout.

What it writes: `manifest.json` at `0.1.0` with the schemas by `$ref` and
the entry, a `description` and a `use_when` to replace; `example.json`, a
payload that passes the payload schema; `sample.json`, a whole review
(`title` and `payload`) that `pinrail submit <name> --sample` and Settings
send; `schemas/` with one property each and a description saying what
to replace; `view/index.html` and `view/view.js`, typed with `// @ts-check`
against `pinrail-plugin.d.ts`, the SDK's types copied beside the manifest
(or `src/`, `vite.config.ts` and `tsconfig.json`), a yes-or-no question
with a comment in the style of the sample plugins; `AGENTS.md`, the plugin
explained to an agent helping build it, and `CLAUDE.md` pointing at it;
`fixtures/basic.json`; `tests/<name>.spec.ts` under the
harness with its `playwright.config.ts`; `package.json` depending on this
package and Playwright; a `.gitignore`; a README with the commands; and
`.github/workflows/release.yml`, which attaches `<name>-<version>.zip` to a
GitHub release on a `v<version>` tag, for `pinrail plugins install
<releases URL>`.

`--dir` puts it somewhere other than `./<name>`. `--sdk` sets where
`package.json` gets this package from; the default is the tarball of the
SDK's own GitHub release.

## Running a plugin in the browser

`pinrail-plugin dev` is a shell for one plugin, without the app: point it at
a plugin directory and it serves the view under the app's CSP with the SDK
beside it, and opens a page that plays the shell.

```sh
npx pinrail-plugin dev .                       # in a plugin folder that has the package installed
mise run dev:plugin plugins/artifact          # in this repository, where nothing at the root links it
                                              # --port N (4790), --no-open
```

The page lists the plugin's `fixtures/*.json` to initialise the view with,
lets a decided fixture stand in as the previous round, toggles read-only and
the theme, sends `collect` the way the app's hand-over button does, and
answers a submit with `violations` you type or with `submitted`. Everything
the view posts — `ready`, `resize`, `draft`, `status`, `submit` — appears in
a log beside it. A change to any file in the plugin reloads the view, with
the last draft handed back on the next `init`, so it pairs with a build in
watch mode. The app can serve the same folder at the same time:
`pinrail plugins install <dir> --link`, which `dev` prints at start.

## Testing a plugin in isolation

`pinrail-plugin/testing` (`harness/`) mounts a plugin directory in a sandboxed
iframe under a fake shell with the SDK and the app's CSP, so a
view is tested alone, without the app or the CLI:

```ts
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";

const plugin = await mountPlugin(page, pluginDir, { review: fixture("fixtures/basic.json") });
await plugin.frame.getByRole("button", { name: "Yes" }).click();
expect(await plugin.nextSubmit()).toEqual({ ok: true });
```

A fixture is part of a review, in the form the SDK passes a view as `review`, usually `{ "title", "payload" }`, or
with a `decision` for a read-only or previous-round case. Tests live in
`<plugin>/tests/*.spec.ts`; `pinrail-plugin test [dir]` runs them, with
Playwright from the plugin's own dependencies and the plugin's
`playwright.config` when it has one (the package's otherwise), and hands
anything else on the line to Playwright: `-g "hands over"`, `--headed`.
In this repository `mise run test:plugins` runs every sample's tests from
`plugins/`, which depends on this package by path.

## Checking a plugin

`pinrail-plugin check [dir]` says what the app's inspect would say, without
the app: the manifest, the name, the version, the entry (or the build that
writes it), the schemas and their `$ref`s are *problems* that refuse the
folder; a `settings_schema`, `shortcuts` list, `decision_template`,
`example`, `sample` or `icon` with the wrong shape, an icon that is not an
SVG file in the folder, an example
that does not pass the payload schema, or a sample without a title, a
passing payload or its files, is a *warning*, the feature the
app drops with the reason on the plugin's row. `--json` gives the same as
data. The rules are the core's, carried in JavaScript; a test in the core
runs both over the same folders and compares. The one thing `check` cannot
do is compile a template: the app does that on install, and rendering a
decided fixture shows the result.

## Types

```ts
import type { Manifest, Init, Review, ShellMessage, PluginMessage } from "@forgeplane/pinrail-plugin/types";
```

`types.d.ts` is the protocol written down: the manifest with every key the
app reads, the envelope a view is handed, the messages both ways, and the
shape of `window.Pinrail`.

## Versions

The package's version is the SDK's (`Pinrail.version`), and its major is the
protocol's: `1.x` serves `sdk/v1`. The app copies `src/` into what it serves
at `/sdk/v1` on every build, so the app and the package carry the same bytes
at the same commit.

## License

The package is licensed under the Apache License 2.0; see `LICENSE` and
`NOTICE`. The files `pinrail-plugin create` writes into a new plugin come from
`templates/`, which is licensed under MIT No Attribution
(`templates/LICENSE`): a plugin made from them is yours to license however you
like, with no notice to keep.
