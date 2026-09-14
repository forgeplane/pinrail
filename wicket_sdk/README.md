# wicket SDK

The plugin side of the wicket protocol, as one dependency-free file the app
serves at `/sdk/v1/wicket-plugin.js`. A plugin loads it with a single script
tag and has the whole handshake done for it: `ready`, origin pinning,
resize, drafts, `submitted`, `violations`, `collect` and the ⌘/Ctrl+Enter
shortcut.

```html
<script src="/sdk/v1/wicket-plugin.js"></script>
<script>
  const plugin = Wicket.connect({
    resize: "auto",                     // "auto" (content height), "fill" (viewport), "manual"
    onInit({ gate, previous, readonly, draft }) { render(); },
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
  plugin.readonly; plugin.gate; plugin.previous;
  plugin.settings;                      // the plugin's own settings, every key the manifest declares
  plugin.setSetting("diff", "split");   // asks the shell to keep one; it comes back as `settings`
</script>
```

The shell owns the hand-over. A view renders no submit button: the shell puts
one next to the note box for every gate, and pressing it sends `collect`. Your
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
on the document, marked `wicketForwarded`, so the listener a view already
has handles a forwarded key like a typed one; `onKey(key)` fires as well.

## Icons

The app serves the [Lucide](https://lucide.dev) set, one file per icon, under
`/sdk/v1/icons/<name>.svg`. `Wicket.icon(name)` returns the markup:

```js
`<button class="btn">${Wicket.icon("check")} Accept</button>`
```

Every name in the set works, and a view downloads only the icons it names, so
the size of the set costs you nothing. The glyph is drawn as a mask, which is
what makes it take `currentColor`: an icon is the colour of the text it sits
in, in either theme, with nothing to configure. Size follows the font size;
`{ size: 18 }` or `{ size: "1.25em" }` overrides it.

An icon is decorative by default and is not announced. Pass `{ label: "delete" }`
when the icon is the only thing saying what a control does. A name with no icon
behind it renders as empty space, with the name left on the element.

The set is ISC licensed and the licence is served beside it at
`/sdk/v1/icons/LICENSE`; the copy in this repository is `licenses/lucide-icons.txt`.

Helpers: `Wicket.escape(s)`, `Wicket.markdown(s)` (paragraphs, bold,
italic, inline and fenced code, lists, http links; escapes first), and
`Wicket.previousVerdict(previous, id)` for decisions shaped as
`{ decisions: [{ id, action, note }], undecided: [id] }`.

`v1` is the protocol major: it only ever gets fixes. The source of truth is
`src/wicket-plugin.js` here; `server/assets` copies it into the app's static
files at build time, so there is exactly one copy in the repository.

## The stylesheet

`src/wicket-plugin.css` is served beside the SDK at
`/sdk/v1/wicket-plugin.css`. It carries the app's tokens for both themes, the
base typography and scrollbars, and a small set of classes for the furniture
every view needs: header and content, items, severity chips, buttons, fields,
notices. A view links it and writes only what is its own.

It is optional and overridable: a view's own `<style>` comes after it. The
list of classes is in [`plugins/README.md`](../plugins/README.md).

`Wicket.layout()` builds the skeleton the stylesheet expects and returns its
elements, so a view can rewrite its body on every change while the header and
its controls stay put:

```js
const view = Wicket.layout({ title: "5 items", controls: [button] });
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
npm test            # Node's test runner, against a fake shell environment
```

## Running a plugin in the browser

`testing/serve.mjs` is a shell for one plugin, without the app: point it at
a plugin directory and it serves the view under the app's CSP with the SDK
beside it, and opens a page that plays the shell.

```sh
node wicket_sdk/testing/serve.mjs ./plugins/artifact --open   # or: mise run dev:plugin plugins/artifact
```

The page lists the plugin's `fixtures/*.json` to initialise the view with,
lets a decided fixture stand in as the previous round, toggles read-only and
the theme, sends `collect` the way the app's hand-over button does, and
answers a submit with `violations` you type or with `submitted`. Everything
the view posts — `ready`, `resize`, `draft`, `status`, `submit` — appears in
a log beside it. A change to any file in the plugin reloads the view, with
the last draft handed back on the next `init`, so it pairs with a build in
watch mode. Node is the only requirement.

## Testing a plugin in isolation

`testing/playwright.ts` mounts a plugin directory in a sandboxed iframe under
a fake shell (`testing/harness.html`) with the SDK and the app's CSP, so a
view is tested alone, without the server or the CLI:

```ts
import { fixture, mountPlugin } from "../../../wicket_sdk/testing/playwright";

const plugin = await mountPlugin(page, pluginDir, { gate: fixture("fixtures/basic.json") });
await plugin.frame.getByRole("button", { name: "Yes" }).click();
expect(await plugin.nextSubmit()).toEqual({ ok: true });
```

A fixture is a partial gate envelope, usually `{ "title", "payload" }`, or
with a `decision` for a read-only or previous-round case. Tests live in
`<plugin>/tests/*.spec.ts` and run with `mise run test:plugins`.
