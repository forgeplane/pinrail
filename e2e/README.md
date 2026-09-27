# End-to-end tests

Where the pieces are proven to agree: the desktop app's server run headless,
the real CLI binary, and the app's UI in a real browser, driven by
[Playwright Test](https://playwright.dev/docs/intro).

| Suite | Checks |
|---|---|
| `tests/cli.spec.ts` | the CLI against the server: submit with wait, two waiters, withdraw, discard, timeout, refusal, markdown, installing plugins |
| `tests/preview.spec.ts` | a review's preview in a browser: the view fed the review, the hand-over checked, nothing decided |
| `tests/zz-restart.spec.ts` | a wait survives the server being killed and restarted |

The shell suites run the app's UI from Vite against a headless server (`shell.config.ts`, `mise run e2e:shell`):

| Suite | Checks |
|---|---|
| `shell/sandbox.spec.ts` | a plugin view's sandbox: no network, no storage, no reach into the shell, and its own files sandboxed when opened as a page |
| `shell/attachments.spec.ts` | a view gets the files its review carries, and only those |
| `shell/review-switch.spec.ts` | moving between reviews loads the right view and carries nothing over |
| `shell/new-round.spec.ts` | a new round of the open review shows in the switcher |
| `shell/inbox-layout.spec.ts` | the inbox grouped by project or as one list |
| `shell/pagination.spec.ts` | the inbox and history page through 50 at a time |
| `shell/sidebar.spec.ts` | the sidebar's waiting list and walking it with ⌥↓ |
| `shell/settings-data.spec.ts` | the data settings: days to keep history, the port |
| `shell/settings-plugins.spec.ts` | the plugins section: installing from a folder, the build command as the consent |
| `shell/theme.spec.ts` | ⌘⇧L switches the theme, and T is left to plugins |

Plugin behaviour in isolation is not here: each plugin has `tests/` mounted
under the `pinrail-plugin` harness, run from `plugins/` with
`mise run test:plugins`.

Global setup builds the CLI and the desktop app if needed, starts the server
with `pinrail serve` (the app run `--headless`) on a scratch data dir and a free
port, and links `plugins/`. Teardown stops it. Runs are explicit:

```sh
mise run e2e                     # the CLI suites, from anywhere in the repo
mise run e2e:shell               # the UI suite
# or
cd e2e && npm install && npx playwright install chromium && npm test
```

Use `npm run test:headed` to watch. Traces for failed tests land in
`test-results/`. Set `PINRAIL_CLI` or `PINRAIL_DESKTOP_BIN` to test a specific
binary instead of a fresh debug build.
