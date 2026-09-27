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
| `tests/preview.spec.ts` | a review's preview in a browser: the view receives the review, and handing over checks the decision without deciding anything |
| `tests/zz-restart.spec.ts` | a wait that continues when the server is stopped and started again |

The global setup builds the CLI and the desktop app if needed, starts the
server with `pinrail serve` on a scratch data directory and a free port, and
links the plugins in `plugins/`. The teardown stops the server.

## The app suites

The app suites run the app's UI from Vite against a headless server
(`shell.config.ts`, `mise run e2e:shell`):

| Suite | What it checks |
|---|---|
| `shell/decide.spec.ts` | deciding in a plugin's view and handing over, a refused decision, forwarded shortcuts, drafts across a reload, a review withdrawn while open, and a CLI waiter receiving the decision |
| `shell/sandbox.spec.ts` | a plugin view's sandbox: no network, no storage and no access to the app, including the plugin's own files opened as a page |
| `shell/attachments.spec.ts` | a view receives the files its review carries, and only those |
| `shell/review-switch.spec.ts` | moving between reviews loads the right view and carries nothing over |
| `shell/new-round.spec.ts` | a new round of the open review appears in the round switcher |
| `shell/inbox-layout.spec.ts` | the inbox grouped by project or shown as one list |
| `shell/pagination.spec.ts` | the inbox and the history in pages of 50 |
| `shell/palette.spec.ts` | the command palette finds reviews of every status |
| `shell/sidebar.spec.ts` | the sidebar's list of waiting reviews, and moving through it with ⌥↓ |
| `shell/settings-data.spec.ts` | the data settings: how long history is kept, and the port |
| `shell/settings-plugins.spec.ts` | the plugins settings: installing from a folder, with the build command shown for consent |
| `shell/theme.spec.ts` | ⌘⇧L switches the theme, and T is left to plugins |

Each plugin's behaviour on its own is tested in the plugin's `tests/`
folder, under the `pinrail-plugin` harness. Run those with
`mise run test:plugins`.

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
