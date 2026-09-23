# End-to-end tests

Where the pieces are proven to agree: the desktop app's server run headless,
the real CLI binary, and the app's UI in a real browser, driven by
[Playwright Test](https://playwright.dev/docs/intro).

| Suite | Checks |
|---|---|
| `tests/cli.spec.ts` | the CLI against the server: submit with wait, two waiters, withdraw, discard, timeout, refusal, markdown, installing plugins |
| `tests/zz-restart.spec.ts` | a wait survives the server being killed and restarted |
| `shell/*.spec.ts` | the app's UI from Vite against a headless server: settings, installing plugins, the sidebar, switching reviews, paging (`shell.config.ts`, `mise run e2e:shell`) |

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
