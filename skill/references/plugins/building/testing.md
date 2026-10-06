# Testing a plugin

A plugin's tests run its view alone, in a browser, without the app or the
CLI. They use Playwright and the test harness of the plugin SDK,
`pinrail-sdk/testing`, which serves the folder as the app does and plays
the app's side of the conversation with the view.

## Set it up

Create the plugin with tests:

```sh
pinrail plugins new <name> --dir <path> --playwright   # with --template vite or react as well
```

This adds `package.json`, `playwright.config.ts` and a first test,
`tests/<name>.spec.ts`. Then, in the folder:

```sh
npm install
npx playwright install chromium   # once, the browser the tests run in
npm test                          # builds a view that has a build, then runs tests/
```

To add tests to a plugin that has none, create a plugin with
`--playwright` in another folder, and copy its `playwright.config.ts`,
`tests/`, and the `test` script and development dependencies of its
`package.json`.

## Write a test

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
- Every call is in `pinrail-sdk/testing`'s types.

Test what the person does and what the agent gets: each control, the
decision it produces, an answer that is not complete yet, and the
read-only view of a decided review. A decided fixture,
`fixtures/<name>.decided.json`, is a review with its `decision`, which
`pinrail plugins check` also checks against the schemas.
