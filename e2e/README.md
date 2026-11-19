# End-to-end tests

These tests run the parts of Pinrail together: the desktop app's server
running headless, the real `pinrail` binary, and the app's UI in a browser.
They use [Playwright Test](https://playwright.dev/docs/intro).

## The CLI suites

The CLI suites run the `pinrail` command against the server
(`playwright.config.ts`, `mise run e2e`):

| Suite                      | What it checks                                                                                                                                                                    |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tests/cli.spec.ts`        | submitting and waiting, two waiters, withdrawing, discarding, timeouts, refused payloads, Markdown output and installing plugins                                                  |
| `tests/preview.spec.ts`    | a review's preview in a browser: the view receives the review, handing over checks the decision without deciding anything, and a view that declares the clipboard copies its text |
| `tests/zz-restart.spec.ts` | a wait that continues when the server is stopped and started again                                                                                                                |

The global setup builds the CLI and the desktop app if needed, starts the
server with `pinrail serve` on a scratch data directory and a free port,
links the email, hello and review plugins from `plugins/`, and builds and
installs a copy of the artifact plugin. The teardown stops the server.

## The app suites

The app suites run a production build of the app's UI against a headless
server (`shell.config.ts`). `mise run e2e:shell` runs them with
`run-shell.mjs`, in several shards at once, each with its own server, UI
and browser.

The specs are grouped in folders under `shell/` by the part of the app
they check. The shared fixtures and helpers stay in `shell/` itself. A new
spec goes in the folder of the part it checks:

| Folder            | What its specs check                                                                                                                                                                          |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `shell/app/`      | The window as a whole: the sidebar, the command palette, the keyboard-shortcuts dialog, the title bar, feedback, the person token, and how a page shows a failed load or a screen that throws |
| `shell/inbox/`    | The inbox: its tabs, its layout, its Agents section, its pages, and its live updates                                                                                                          |
| `shell/reviews/`  | A review in the app: the review screen and its side panel, its breadcrumb, stars, deleting, dictation, find, the history, and what a review says it asks and decided                          |
| `shell/plugins/`  | Plugin views and the plugin protocol: deciding and handing over, the sandbox, attachments, the clipboard, links, rounds, linking a plugin in development, and the official plugins' fixtures  |
| `shell/sessions/` | An agent's sessions and their threads: messages, notes, questions, the session's header, the Sessions page and the sidebar's sessions                                                         |
| `shell/runtime/`  | Agents Pinrail runs: starting one, its terminal, Resume, the banner after a restart, and the terminal's WebAssembly                                                                           |
| `shell/items/`    | Work items and projects: the Work items page, an item's side panel, the quick add, and items named elsewhere                                                                                  |
| `shell/settings/` | Settings and the setup: each section of Settings, themes, the first-run setup, the usage data notice, and What's new                                                                          |

Each plugin's behaviour on its own is tested in the plugin's `tests/`
folder, under the `pinrail-sdk` harness. Run those with
`mise run test:plugins`.

## Selecting elements

The app suites find an element the way a person finds it. Use the first of
these that works:

1. Its role and accessible name, with `getByRole`, such as
   `getByRole("button", { name: "Next page" })`.
2. Its label, placeholder or visible text, with `getByLabel`,
   `getByPlaceholder` or `getByText`.
3. A test id, with `getByTestId`.

A role query also checks accessibility. If a control cannot be found by its
role and name, a screen reader cannot announce it properly either. Fix the
markup in that case, for example with an `aria-label` or a `role`, rather
than adding a test id.

Use a test id only for an element that has no role or name to find it by.
Examples are a row whose accessible name is all of its text, a container
that a spec searches inside, and a block of text whose wording the spec
checks. The app's element then carries a `data-testid` attribute:

- The id names what the element is, not how it looks, in kebab case, such
  as `review-row` or `install-panel`.
- An element that is repeated once per item ends with the item's name, such
  as `plugin-row-greeter`, so a spec can find one item directly.
- An element inside such an item has a plain id, such as `plugin-details`,
  and a spec finds it within the item:
  `getByTestId("plugin-row-greeter").getByTestId("plugin-details")`.

Test ids exist only for the tests. App code does not read them. Add a test
id in the same change as the test that uses it, and remove it when no test
uses it any more.

A spec does not find an element by its class name or by the structure of
the page, because those change with the styling. To check a state, prefer
the ARIA attribute or the matcher that reads it, such as `aria-checked`,
`aria-expanded` or `toBeDisabled()`. Where the app shows a state only
through a class, a spec may check that class, such as
`toHaveClass(/is-focused/)`.

A plugin's view is the exception. A spec finds it with
`page.frameLocator("#plugin-frame")`, the same frame id that the SDK's test
harness uses, and finds the elements inside the view by the selectors of
the plugin that draws it.

A locator that several specs need belongs in `shell/helpers.ts`, such as
`settingsDialog(page)` for the settings dialog and
`settingsSection(page, "Data")` for a section's button in its rail.

## Running the tests

```sh
mise run e2e                     # the CLI suites, from anywhere in the repository
mise run e2e:shell               # the app suites
# or
cd e2e && npm install && npx playwright install chromium && npm test
```

Add `--headed` to watch the browser. No trace is recorded unless you ask for
one: add `--trace retain-on-failure` to keep the traces of failed tests in
`test-results/`. In CI, a test that fails runs again with a trace. To test a
particular binary instead of a fresh debug build, set `PINRAIL_CLI` or
`PINRAIL_DESKTOP_BIN`.
