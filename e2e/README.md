# End-to-end tests

The only place the pieces are proven to agree: a real server, the real CLI
binary and a real browser, driven by [Playwright Test](https://playwright.dev/docs/intro).
Suites are organised by what they check, not by language:

| Suite | Checks |
|---|---|
| `tests/cli.spec.ts` | the CLI against the server: create with wait, two waiters, withdraw, timeout, refusal, types |
| `tests/plugins/*.spec.ts` | each shipped plugin, decided in the browser inside its sandboxed frame, as seen by the waiter |
| `tests/zz-restart.spec.ts` | a wait survives the server being killed and restarted |
| `shell/*.spec.ts` | the desktop shell in a browser against the headless desktop server: installing a plugin from Settings, its rows (`shell.config.ts`, `mise run e2e:shell`) |

Plugin behaviour in isolation is not here: each plugin has `tests/` mounted
under the `wicket-plugin` harness, run from `plugins/` with
`mise run test:plugins`.

Global setup builds the CLI if needed, starts the server with `wicket serve`
on a scratch data dir and a free port, and registers `plugins/`. Teardown
stops it. Runs are explicit:

```sh
mise run e2e                     # from anywhere in the repo
# or
cd e2e && npm install && npx playwright install chromium && npm test
```

Use `npm run test:headed` to watch. Traces for failed tests land in
`test-results/`. Set `WICKET_CLI` to test a specific binary instead of a
fresh debug build.
