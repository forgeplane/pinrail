---
title: Testing a plugin
description: "Tests of a plugin's view on its own, in a browser, with Playwright and the plugin SDK's test harness: no app and no CLI needed."
---

A plugin's tests open its view in a browser, use it the way a person would, and check the decision it hands over. They run without the app or the `pinrail` command, so they also run in CI. They use [Playwright](https://playwright.dev) and the test harness of the plugin SDK, `pinrail-sdk/testing`, which serves the plugin's folder as the app does and plays the app's side of the conversation with the view.

## Set up the tests

Create the plugin with `--playwright`, in any template:

```sh
pinrail plugins new ticket_triage --playwright
cd ticket_triage
npm install
npx playwright install chromium    # once, the browser the tests run in
npm test
```

`--playwright` adds a `package.json` with Playwright and the SDK, [`pinrail-sdk`](https://www.npmjs.com/package/pinrail-sdk) from npm, as development dependencies, a `playwright.config.ts`, and a first test in `tests/ticket_triage.spec.ts`. With `--template typescript` or `--template react`, it adds them to the template's own `package.json`, and `npm test` builds the view before it runs the tests.

To add tests to a plugin that has none, install the two packages, `npm install --save-dev @playwright/test pinrail-sdk`. Then create a plugin with `--playwright` in another folder, and copy its `playwright.config.ts`, its `tests/` folder, and the `test` script of its `package.json`.

None of these files are installed with the plugin. Pinrail copies only the plugin's own files, as [Writing a plugin](/docs/building/writing/#create-the-folder) lists them.

## Write a test

```ts title="tests/ticket_triage.spec.ts"
import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "pinrail-sdk/testing";

const dir = path.resolve(__dirname, "..");
const sample = () => fixture(path.join(dir, "samples", "ticket_triage.json"));

test("hands over the answer", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: sample() });
  await plugin.frame.getByRole("button", { name: "Yes" }).click();
  expect(await plugin.handOver()).toMatchObject({ decision: { data: { ok: true } } });
});

test("asks for an answer before it hands over", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: sample() });
  expect(await plugin.handOver()).toEqual({ deferred: true });
});
```

`mountPlugin(page, dir, options)` opens the view with a review. `options.review` is a sample or a fixture, which `fixture(file)` reads. The other options set what the app would send with it: `readonly` for a review that is no longer pending, `draft`, `settings`, `theme`, `previous` for the round before, and `attachments` for the files the review carries.

The plugin it returns has the view's frame, `plugin.frame`, and the app's side of the conversation:

| Call | What it does |
|---|---|
| `handOver()` | Presses the app's hand-over button, and returns what the app would get: the decision, once the decision schema accepts it, the violations of a decision that the schema refuses, or `{ deferred: true }` when the view returned nothing. |
| `lastDraft()`, `lastStatus()`, `messages()` | What the view sent: its draft, the label of the hand-over button, and every message. |
| `sendKey("j")` | Presses a shortcut the manifest declares, as the app forwards it. |
| `settings({ … })` | Changes the plugin's settings, as *Settings › Plugins* does. |
| `reinit()`, `reload()` | Opens the review again, with the last draft, or reloads the frame. |

The package's types describe every call.

## What to test

Test what the person does and what the agent gets back: each control, the decision it produces, an answer that is not complete yet, and the read-only view of a decided review. Find the view's parts by their role and text, as a person sees them, so the tests do not depend on how the view is built.

A decided fixture, `fixtures/<name>.decided.json`, is a review with its `decision`. The tests can open it read-only, and [`pinrail plugins check`](/docs/building/frameworks/#4-try-it-and-test-it) checks its payload and decision against the plugin's schemas.

To look at the view while you work on it, rather than test it, use the [dev shell](/docs/building/writing/#work-on-the-view-in-the-dev-shell).
