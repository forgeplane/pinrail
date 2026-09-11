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
  });
  plugin.submit(data);
  plugin.draft(data);                   // debounced 150ms; { flush: true } posts at once
  plugin.status({label: "Hand over anyway"});   // what the shell's button should read
  plugin.readonly; plugin.gate; plugin.previous;
</script>
```

The shell owns the hand-over. A view renders no submit button: the shell puts
one next to the note box for every gate, and pressing it sends `collect`. Your
view may submit at once or confirm first and submit on the next `collect`;
`status` keeps the button's label honest.

The shell owns the theme and sends it before `init` and again whenever it
changes. The SDK sets `data-theme` on the plugin's root element and exposes
`plugin.theme`, so a view only has to write the CSS:

```css
:root { --bg: #18191b; --text: #ededef; color-scheme: dark; }
[data-theme="light"] { --bg: #fff; --text: #24262c; color-scheme: light; }
```

A theme change never re-initialises the view or touches its draft.

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

## Tests

```sh
npm test            # Node's test runner, against a fake shell environment
```

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
