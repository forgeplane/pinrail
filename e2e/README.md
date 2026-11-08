# End-to-end tests

These tests run the parts of Pinrail together: the desktop app's server
running headless, the real `pinrail` binary, and the app's UI in a browser.
They use [Playwright Test](https://playwright.dev/docs/intro).

## The CLI suites

The CLI suites run the `pinrail` command against the server
(`playwright.config.ts`, `mise run e2e`):

| Suite | What it checks |
|---|---|
| `tests/cli.spec.ts` | submitting and waiting, two waiters, withdrawing, discarding, timeouts, refused payloads, Markdown output and installing plugins |
| `tests/preview.spec.ts` | a review's preview in a browser: the view receives the review, handing over checks the decision without deciding anything, and a view that declares the clipboard copies its text |
| `tests/zz-restart.spec.ts` | a wait that continues when the server is stopped and started again |

The global setup builds the CLI and the desktop app if needed, starts the
server with `pinrail serve` on a scratch data directory and a free port,
links the email, hello and review plugins from `plugins/`, and builds and
installs a copy of the artifact plugin. The teardown stops the server.

## The app suites

The app suites run a production build of the app's UI against a headless
server (`shell.config.ts`). `mise run e2e:shell` runs them with
`run-shell.mjs`, in several shards at once, each with its own server, UI
and browser:

| Suite | What it checks |
|---|---|
| `shell/decide.spec.ts` | deciding in a plugin's view and handing over, a refused decision, forwarded shortcuts, drafts across a reload, a review withdrawn while open, and a CLI waiter receiving the decision |
| `shell/sandbox.spec.ts` | a plugin view's sandbox: no network, no storage and no access to the app, including the plugin's own files opened as a page |
| `shell/clipboard.spec.ts` | copying a view's text and an image of its review, with a notice for each, for a plugin that declares the `clipboard` capability and for no other |
| `shell/attachments.spec.ts` | a view receives the files its review carries, and only those |
| `shell/review-switch.spec.ts` | moving between reviews loads the right view and carries nothing over |
| `shell/new-round.spec.ts` | a new round of the open review appears in the round switcher |
| `shell/inbox-layout.spec.ts` | the inbox grouped by project or shown as one list |
| `shell/pagination.spec.ts` | the inbox and the history in pages of 50 |
| `shell/palette.spec.ts` | the command palette finds reviews of every status |
| `shell/sidebar.spec.ts` | the sidebar's list of waiting reviews, and moving through it with ⌥↓ |
| `shell/settings-data.spec.ts` | the data settings: how long history is kept, and the port |
| `shell/settings-plugins.spec.ts` | the plugins settings: installing from a folder, and refusing one whose view is not built |
| `shell/theme.spec.ts` | ⌘⇧L switches the theme, and T is left to plugins |
| `shell/conformance.spec.ts` | the app and the preview host a view as the protocol describes, and a decision that arrives after moving on stays with its review |
| `shell/frame-navigation.spec.ts` | a view that leaves its page is no longer answered, and the frame shows only Pinrail's own server |
| `shell/open-links.spec.ts` | a link from a view opens only when the person agrees, or for an origin they allowed for that plugin |
| `shell/live-updates.spec.ts` | the inbox, the history and the Plugins section update from events, fetching only what they show |
| `shell/history-navigation.spec.ts` | a review opened from the history is one step back from it |
| `shell/inbox-cap.spec.ts` | an inbox with more pending reviews than it loads says how many there are |
| `shell/linux-keys.spec.ts` | on Linux, Settings names keys as a Linux keyboard does |

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

Add `--headed` to watch the browser. The traces of failed tests are saved in
`test-results/`. To test a particular binary instead of a fresh debug build,
set `PINRAIL_CLI` or `PINRAIL_DESKTOP_BIN`.
