# Building a plugin

A plugin is a folder: what the agent sends, what the person sees, and the
decision the agent gets back. This guide goes from a new folder to a
review in the person's inbox, and then covers the view's design, settings
and keys, and frameworks.

## 1. Create the plugin

```sh
pinrail plugins new <name> --dir <path>
```

This writes a working plugin that asks one yes-or-no question. It needs
no build and installs nothing. Two options change what it writes:

- `--template typescript` (or `ts`) or `--template react` writes a view
  in TypeScript or React in `src/`, built by Vite into `view/`. See
  [frameworks](#frameworks).
- `--playwright` adds a first test of the view. See
  [test it](#8-test-it).

## 2. The files

Each file has a fixed place, which the manifest does not name. An install
copies these files and nothing else:

- `manifest.json`: `name` and `version` are required. Always give
  `use_when` as well: the moment an agent should ask with the plugin,
  stated precisely, because agents choose by it. `summary` declares what
  the inbox counts, such as items by severity. `pinrail plugins schema`
  prints the manifest's JSON Schema, with every key.
- `schemas/decision.schema.json`: what the view hands back.
- `schemas/payload.schema.json`: what the agent sends.
- `view/index.html`, with its scripts, styles and icons in `view/`.
- `samples/<name>.json`: whole reviews that show the plugin at work.
- `icon.svg`, `README.md`, and optionally `LICENSE` and
  `templates/decision.md.j2`.

`pinrail-plugin.d.ts`, the SDK's types, is for your editor while you work,
and is not installed. `pinrail plugins check <path>` reports what the app
would refuse in the folder, and each feature it would drop.

## 3. Design the decision

Start with `schemas/decision.schema.json`. Shape it around the exact
action you will take on the answer, such as approve, send back with
changes, or stop, and say what an incomplete answer means. A view that
shows the right material but returns another kind of answer does not do
the job.

The app lets the person add a note to the agent with any decision, so do
not add a comment field for the whole review. Add a note field only to a
part that is answered on its own, such as an item or a passage.

## 4. The payload and the samples

Then write `schemas/payload.schema.json`. The view can load nothing from
outside the folder, so the payload carries everything it shows. Both
schemas are JSON Schema 2020-12.

A sample is a whole review: a `title`, a `payload`, and any
`attachments`, relative to `samples/`. The first sample's payload is also
the example that `pinrail plugins describe` gives agents, so keep it
small. A change to the payload's shape touches the payload schema, the
view and the samples together.

To take files beside the payload, such as images or documents, declare
them in the manifest's `attachments`, with the kinds the plugin accepts.
The payload names each file as `{ "$attachment": "<name>" }`. Describe
such a field in the payload schema with a copy of the attachment schema,
which `pinrail plugins schema attachment` prints. `pinrail plugins check`
warns when the manifest and the payload schema disagree about files.

## 5. The view

The view is a web page in `view/index.html`. It loads the SDK from the
app, and talks to the app only through it:

```html
<link rel="stylesheet" href="/sdk/v1/pinrail-plugin.css">
<script src="/sdk/v1/pinrail-plugin.js"></script>
<script src="view.js"></script>
```

```js
const plugin = Pinrail.connect({
  onInit({ review, readonly, draft, settings }) {}, // review.payload is what the agent sent
  onCollect() { return decision; },                 // the person pressed the hand-over
});
plugin.draft(value);                    // keeps what the person entered across reloads
plugin.handOverLabel("Hand over: yes"); // the words on the hand-over button
```

- `onInit` draws the review. It runs again, read-only, when the review
  ends while the view is open, for example when the agent withdraws it,
  so draw the view from scratch each time.
- The app draws the hand-over button, and the view never draws its own.
  `onCollect` returns the decision, shaped by the decision schema, or a
  promise of it. It returns nothing while the view needs more from the
  person, and the next press asks again.
- The app checks the decision against the schema. It lists a refused
  decision's violations under the view, and closes the view once a
  decision is accepted. A view that marks the field at fault itself can
  use `onViolations(errors)`, and one that redraws itself `onSubmitted()`.
- `plugin.readonly` is true for a review that is no longer pending. Show
  `review.decision.data`, and offer no editing.
- The frame loads nothing from outside the plugin folder, so the payload
  carries everything the view shows.
- Files: show a file the payload names with
  `await plugin.attachmentUrl(name)`, or read its bytes with
  `await plugin.attachment(name)`.
- Links: `plugin.open(url)`. The app asks the person before it opens a
  link, unless they allowed that site for the plugin.
- Rendering: `Pinrail.escape(text)`, `Pinrail.icon(name)`, and
  `Pinrail.markdown(text)` once the page also loads
  `<script src="/sdk/v1/markdown.js"></script>` after the SDK.
- Layout: the view fills the review panel's height and scrolls inside its
  frame. `Pinrail.layout()` keeps a header in place over a body that
  scrolls.
- Every call and its arguments are in `pinrail-plugin.d.ts`.

The view runs in a sandboxed frame with an opaque origin, so a browser
tool that reads the page's DOM or accessibility tree sees the frame as
empty, even when the view is drawn. To check the view yourself, take a
screenshot, or drive it with Playwright and reach inside with
`page.frameLocator("iframe")`.

## 6. Work on the view with the person

The plugin SDK's dev shell shows the view in a browser, inside a stand-in
for the app, and reloads it whenever a file of the plugin changes. Work on
the view there with the person. It is quicker than sending reviews, and
the person can comment on any part of the view for you to revise.

Serve the plugin with the SDK of the same version as this Pinrail, and
give the person the address it prints, `http://127.0.0.1:4790` unless you
pass `--port`. It needs Node, and nothing installed in the folder. Keep it
running while you work.

```sh
npx pinrail-sdk@{{sdk_version}} dev <path> --no-open
```

- The bar picks the review the view opens with, from `samples/` and
  `fixtures/`, a decided one as the previous round, and read-only. It also
  switches the theme.
- *JSON* shows the payload, the payload schema and the decision schema in
  place of the view.
- The side panel plays the app's part: the hand-over, the settings and
  keys the manifest declares, and what the view sent, such as its draft
  and its decision, checked against the decision schema.

The person turns on *Select*, or presses I, clicks a part of the view, and
writes what should change there. *Copy comments* puts all the comments on
the clipboard as Markdown, with the part of the view each one is about,
and the person pastes them to you. Make the changes, and the view reloads
by itself. The person then clears the comments in the shell, and comments
again. Repeat until the person is satisfied with the view.

## 7. Try it in the app

```sh
pinrail plugins check <path>              # what the app would refuse, and why
pinrail plugins install <path> --link     # the app serves the folder as it changes
pinrail submit <name> --sample            # a real review of the first sample
pinrail withdraw <id>                     # when you are done with the sample
```

A linked plugin is listed in the person's plugins until
`pinrail plugins remove <name>`. Linking a plugin that the person's task
needs is part of that task. A change to the manifest or a schema applies
within a second. A view with a build needs `npm run build`, or
`npm run watch` while you work, before the app serves the change.

## 8. Test it

A plugin's tests run its view alone, in a browser, without the app or the
CLI. They use Playwright and the test harness of the SDK,
`pinrail-sdk/testing`, which serves the folder as the app does and plays
the app's side of the conversation.

A plugin created with `--playwright` has `package.json`,
`playwright.config.ts` and a first test, `tests/<name>.spec.ts`. To add
tests to a plugin that has none, create a plugin with `--playwright` in
another folder, and copy its `playwright.config.ts`, `tests/`, and the
`test` script and development dependencies of its `package.json`. Then,
in the folder:

```sh
npm install
npx playwright install chromium   # once, the browser the tests run in
npm test                          # builds a view that has a build, then runs tests/
```

```ts
import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "pinrail-sdk/testing";

const dir = path.resolve(__dirname, "..");

test("hands over the answer", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: fixture(path.join(dir, "samples", "<name>.json")) });
  await plugin.frame.getByRole("button", { name: "Yes" }).click();
  expect(await plugin.handOver()).toMatchObject({ decision: { data: { ok: true } } });
});
```

- `mountPlugin(page, dir, options)` opens the view with a review.
  `options.review` is a sample or a fixture, as `fixture(file)` reads it.
  `readonly`, `draft`, `settings`, `theme`, `previous` and `attachments`
  set the rest of what the app would send.
- `plugin.frame` is the view's frame. Find its parts by role and text, as
  a person sees them.
- `plugin.handOver()` does what the app's hand-over button does. It
  returns the accepted decision, the violations of one that the decision
  schema refuses, or `{ deferred: true }` when the view returned nothing.
- `plugin.lastDraft()`, `plugin.lastStatus()` and `plugin.messages()`
  read what the view sent. `plugin.sendKey("j")` presses a declared
  shortcut, and `plugin.settings({ … })` changes the plugin's settings.

Test what the person does and what the agent gets: each control, the
decision it produces, an answer that is not complete yet, and the
read-only view of a decided review. A decided fixture,
`fixtures/<name>.decided.json`, is a review with its `decision`, which
`pinrail plugins check` also checks against the schemas.

## 9. Change it later

- A release whose schemas refuse what the previous one accepted needs a
  new major version, such as `2.0.0`, or `0.4.0` after `0.3.x`.
  `pinrail plugins check <dir> --since <previous release dir>` lists such
  breaks.
- An opened pending review moves to a new release that accepts its
  payload, and an ended one keeps its release. On a decision you did not
  expect, describe the plugin again.
- To change an installed plugin, link your copy under the same name. It
  takes the installed plugin's place.

## Design

A view looks like part of the app when it uses the app's stylesheet,
`/sdk/v1/pinrail-plugin.css`. It gives the base styles, the colours and the
classes, in the light and the dark theme. `/sdk/v1/tokens.css` gives the
colours alone. Every rule is in the cascade layer `pinrail`, so any rule of
the view's own wins. Style with the tokens, never with colour values, and
the view follows the app's theme.

- Colours: `--pinrail-` followed by `bg`, `bg-panel`, `bg-raised`,
  `bg-hover`, `border`, `border-strong`, `text`, `dim`, `faint`, `accent`,
  `accent-bg` or `button-bg`. The tones are `danger`, `warning`, `info`,
  `success` and `neutral`, and diffs have `add-bg`, `add-gut`, `del-bg`
  and `del-gut`.
- Type: `--pinrail-sans` and `--pinrail-mono`, with text at 13px.
- Themes: the root element has `data-theme="dark"` or `"light"`. Key any
  colour of your own on it. `onAppearance(theme)` says when it changes.
- Layout: `Pinrail.layout({ title, meta, controls })` builds a fixed
  `.pinrail-header` over a scrolling `.pinrail-content`. Render into
  `layout.content`.
- Items: `.pinrail-item`, with `.pinrail-item-head`, `-id`, `-title`,
  `-body` and `-controls`.
- Buttons: `.pinrail-btn`, with `-primary`, `-danger` or `-ghost`. Mark the
  chosen one with `aria-pressed="true"`.
- Fields and notices: `.pinrail-field`, `.pinrail-note`, and
  `.pinrail-notice` with `-success`, `-warning` or `-danger`.
- Text: `.pinrail-tone` with a tone, such as `.pinrail-tone-warning`, and
  `.pinrail-chip`, `.pinrail-eyebrow`, `.pinrail-dim`, `.pinrail-faint`,
  `.pinrail-empty` and `.pinrail-errors`.
- Icons, without a build: `Pinrail.icon(name)` draws
  `view/icons/<name>.svg` in the text's colour. Bring the icons you use.
  The app's icons are Lucide, from `lucide-static`, and fit best.
- Icons, with a build: import them from `lucide-react`, `@lucide/vue`,
  `@lucide/svelte`, or `lucide` with
  `createElement(icon, { class: "lucide" })`. The stylesheet sizes an
  `svg.lucide` to the text.
- Put fonts, styles and images of your own in the plugin folder, and refer
  to them by relative path. The plugin's own icon is `icon.svg`.

The app draws the review's title and the hand-over button, so the view
draws neither. Keep the view dense and quiet, with one accent colour and
lines rather than boxes. A decided review shows what was there and what
was decided, without controls.

## Settings and keys

A plugin's settings are a JSON Schema in the manifest's `settings_schema`,
one level deep. Each property is a `boolean`, a `string` (with `enum`, or
`oneOf` with `const` values), an `integer` or a `number`, and needs a
`default`. Its `title` is the label. The app draws a row for each setting
in *Settings › Plugins*.

```js
plugin.settings.diff;                     // the current value, from init and every change
await plugin.setSetting("diff", "split"); // the app checks it and tells every open view
```

`onSettings(settings)` says when the settings change.

List the view's keyboard shortcuts in the manifest's `shortcuts`:

```json
"shortcuts": [
  { "keys": "j", "does": "Next item" },
  { "keys": "a", "does": "Accept the focused item", "group": "Verdicts" }
]
```

- `keys` is any modifiers (`cmd`, `ctrl`, `alt`, `shift`) joined by `+`,
  then one key, named as `KeyboardEvent.code` names it but without `Key`
  or `Digit`: `j`, `1`, `enter`, `arrowdown`. `cmdorctrl` is cmd on macOS
  and ctrl on Linux.
- The app lists the shortcuts in its keyboard help. When the app, not the
  frame, has the focus, it forwards each one to the view as a `keydown`.
- The app keeps some keys for itself: `?`, `[`, `]`, `escape` and
  `cmd+enter` on the review screen, the keys of its menus (`cmd+k`,
  `cmd+,`, `cmd+i`, `cmd+b`, `cmd+[`, `cmd+]`, `cmd+shift+h`,
  `cmd+shift+p`, `cmd+shift+m`, `cmd+shift+l`, `cmd+w`, `cmd+m`,
  `cmd+q`), and the editing keys (`cmd+z`, `cmd+shift+z`, `cmd+x`,
  `cmd+c`, `cmd+v`, `cmd+a`). On Linux, ctrl takes the place of cmd. The
  app forwards every other declared key.

## Frameworks

A view can be built with React, Vue, Svelte or TypeScript, as long as the
build writes an HTML page and its files into the plugin folder.
`pinrail plugins new --template react` or `--template typescript` writes a
working plugin in React or TypeScript, with a Vite build. For Vue or
Svelte, start from the TypeScript plugin, and follow the matching version
of the Ship it? example in `docs/examples/ship-it/` of the Pinrail
repository.

The view is in `src/`, and takes the SDK's types from
`pinrail-plugin.d.ts` in the folder. Run `npm install` once, then
`npm run build`, or `npm run watch` while you work.

- The build writes the page to `view/index.html`, with its scripts and
  styles beside it in `view/`.
- Pinrail installs a plugin as it is and runs nothing, so run the build
  before you install, link or zip the folder.
- The build refers to its files by relative path. With Vite, set
  `base: "./"`.
- The page loads `/sdk/v1/pinrail-plugin.js` and its stylesheet with tags,
  rather than bundling them.
- The build bundles everything else, including the framework, fonts and
  images, because the view cannot load anything from the network.
