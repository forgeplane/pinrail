# Contributing

Thank you for your interest in Pinrail. The project is in early development,
and bug reports are the most useful contribution right now, especially from
real use with an agent.

## What we accept

For now we only accept pull requests that fix bugs. For a new feature or a
change in behaviour, open an issue first.

- **Bug reports:** describe what you ran, what you expected and what
  happened instead. Include your operating system, the app's version (shown
  in *Settings › About*), and the output of the `pinrail` command run with
  `-v`. If you can fix the bug, a pull request is welcome.
- **Suggestions:** open an issue that describes the problem first, then the
  change you would like to see.
- **Security problems:** please do not open an issue. Follow
  [SECURITY.md](SECURITY.md) instead.

## Plugins

Plugins live in their own repositories, and people install them from there.
This repository does not accept new plugins: the `plugins/` folder only holds
the official plugins that ship with the app. To write your own, see
[Writing a plugin](https://pinrail.dev/docs/building/writing/).

## Setting up

The tool versions are pinned in `mise.toml`. With [mise](https://mise.jdx.dev)
installed, run:

```sh
mise install
mise run dev:desktop        # the app with live reload
```

The repository contains:

- `desktop/`: the desktop app
- `cli/`: the `pinrail` command
- `pinrail-plugin/`: the plugin SDK
- `plugins/`: the official plugins
- `e2e/`: the end-to-end tests
- `docs/`: the documentation
- `website/`: the website

## Running the tests

`mise run test` runs every suite below, one after the other. While you work,
run the suite for the part you changed. The browser suites use
[Playwright](https://playwright.dev) with Chromium, which you install once
with `npx playwright install chromium` from `e2e/`.

- **Desktop core and UI:** `mise run test:desktop`. The Rust tests of the
  server (storage, plugins, validation, the HTTP API) and of the app, then a
  type check of the UI. The core's check test compares the Rust plugin check
  with the SDK's, so it needs Node; set `PINRAIL_SKIP_NODE=1` to skip it. One
  test builds a plugin with `npm ci` and needs the network, so it only runs
  when asked: `cargo test -p pinrail-core -- --ignored` from `desktop/`.
- **CLI:** `mise run test:cli`. The `pinrail` command against a scripted
  HTTP server, with no app running.
- **Plugin SDK:** `mise run test:sdk`. Unit tests of `pinrail-plugin` under
  Node, then browser tests of its harness, its `dev` server and the plugins
  its `create` command writes.
- **Official plugins:** `mise run test:plugins`. Each plugin in `plugins/`
  on its own, in the SDK's test harness. The plugins that have a build are
  built first.
- **Docs examples:** `mise run test:examples`. The example plugins in
  `docs/examples/ship-it`, each built and tested, plus a check that the four
  framework versions share everything except the view's code.
- **End to end, CLI:** `mise run e2e`. The real `pinrail` command against the
  app's server running headless on a scratch data folder and a free port.
  Both are built first if needed.
- **End to end, app:** `mise run e2e:shell`. The app's UI in a browser
  against a headless server: the inbox, reviews, deciding in a plugin's
  view, settings and installing plugins. It uses ports 4799 and 5199, which
  must be free.

To run one test, go to the suite's folder and name it:

```sh
cargo test deciding_validates               # Rust: tests whose name contains this
npx playwright test shell/decide.spec.ts    # Playwright: one file
npx playwright test -g "survives a reload"  # Playwright: tests whose title contains this
```

In `e2e/`, add `-c shell.config.ts` to run the app suite. Add `--headed` to
watch the browser. When a Playwright test fails, its trace is saved under
`test-results/`; open it with `npx playwright show-trace`.

Some tests compare output with recorded files. When a change is meant to
alter that output, run the tests with `UPDATE_FIXTURES=1` to rewrite the
files, and check the difference before you commit it.

## Before opening a pull request

Run the linters and the tests:

```sh
mise run lint               # rustfmt, clippy, the type check, licences
mise run test               # all the test suites
```

CI runs the same checks on macOS and Linux, and they must pass before a pull
request can be merged.

- **Tests:** add a test that fails without your fix.
- **Docs:** if your change affects what users see, update the relevant page
  in `docs/`. The pages in `docs/reference/` are generated, so change their
  source and run `mise run docs:generate`.
- **Commits:** keep each commit to one change. Write the title as a sentence
  that says what the commit does, without an `area:` prefix. For example:
  *Let the artifact plugin take its page as an HTML file*.

## License

Pinrail is licensed under the [Apache License 2.0](LICENSE). By contributing,
you agree that your contributions are licensed under the same terms.
