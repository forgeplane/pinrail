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
    onCollect() { submit(); },                     // ⌘/Ctrl+Enter, from the shell or in here
  });
  plugin.submit(data);
  plugin.draft(data);                   // debounced 150ms; { flush: true } posts at once
  plugin.readonly; plugin.gate; plugin.previous;
</script>
```

Helpers: `Wicket.escape(s)`, `Wicket.markdown(s)` (paragraphs, bold,
italic, inline and fenced code, lists, http links; escapes first), and
`Wicket.previousVerdict(previous, id)` for decisions shaped as
`{ decisions: [{ id, action, note }], undecided: [id] }`.

`v1` is the protocol major: it only ever gets fixes. The source of truth is
`src/wicket-plugin.js` here; `server/assets` copies it into the app's static
files at build time, so there is exactly one copy in the repository.

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
