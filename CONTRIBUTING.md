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
This repository does not accept new plugins: the `plugins/` folder holds the
official plugins the app carries, `list`, `feedback`, `code-review`, `image`
and `markdown`, and `hello` and `sampler`, which the tests use. To write your own, see
[Writing a plugin](https://pinrail.dev/docs/building/writing/).

The sample plugins are developed in
[forgeplane/pinrail-plugins](https://github.com/forgeplane/pinrail-plugins).
To work on one's view in the running app, link its folder from a checkout of
that repository:

```sh
pinrail plugins install plugins/code-review --link
```

The link takes the place of the installed plugin of that name, including
the copy of `list` or `feedback` that the app carries. When you change the
folder, an open review of the plugin offers *Reload*. A plugin with a
build step, such as `artifact`, needs `npm run build` after each change.
Remove the link to go back: `pinrail plugins remove review`.

## Setting up

Node is pinned in `mise.toml`, and Rust in `rust-toolchain.toml`, which
[rustup](https://rustup.rs) reads on its own. With rustup and
[mise](https://mise.jdx.dev) installed, run:

```sh
mise install
mise run setup              # the npm packages, the UI and plugin builds, and Playwright's browser
mise run dev:desktop        # the app with live reload
```

`mise run setup` performs the same preparation as CI. The tests and the
linters need it, so run it once after cloning, and again when a
`package.json` or `package-lock.json` changes.

The repository contains:

- `desktop/`: the desktop app
- `cli/`: the `pinrail` command
- `sdk/`: the plugin SDK
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
  type check of the UI and its unit tests (Vitest, for the UI's pure logic:
  formatting, key handling, how events change the pending list). The UI's
  screens are tested end to end, by `mise run e2e:shell`. The core's check test compares the Rust plugin check
  with the SDK's, so it needs Node; set `PINRAIL_SKIP_NODE=1` to skip it.
- **CLI:** `mise run test:cli`. The `pinrail` command against a scripted
  HTTP server, with no app running.
- **Plugin SDK:** `mise run test:sdk`. Unit tests of `pinrail-sdk` under
  Node, then browser tests of its harness and its `dev` server.
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

Two desktop core tests compare their output with recorded files, and can
rewrite them:

- the Markdown an agent gets for a decided review: each
  `plugins/*/fixtures/*.decided.json` against the `.decided.md` beside it;
- the server's answer to a new review, against
  `desktop/core/tests/fixtures/api/create-ok.txt`.

When a change is meant to alter that output, run `UPDATE_FIXTURES=1 cargo
test` from `desktop/`, and check the difference before you commit it. The
other recorded answers in `tests/fixtures/api`, the wording of refusals, are
edited by hand.

## Trying the Linux app from macOS

With Docker installed, run:

```sh
mise run linux-desktop
```

This builds the app and the `pinrail` command for Linux in a container,
and runs the app on a virtual display with a panel, a system tray and a
notification daemon. Open
<http://localhost:6080/vnc.html?autoconnect=1&resize=scale> to see and use
it. The first run compiles everything and takes several minutes; later
runs reuse the build.

The container builds from a copy of your checkout, so your working tree
stays as it is. [docker/linux-desktop/README.md](docker/linux-desktop/README.md)
explains how to send reviews and samples, install plugins and try a
change.

The container has no GPU, and it runs Linux for your Mac's processor
rather than the x86-64 of the release, so check anything that depends on
those on a real Linux machine.

## Before opening a pull request

Run the linters and the tests. ESLint and Prettier come from the package at
the root of the repository, so run `npm ci` there first:

```sh
mise run lint               # rustfmt, clippy, the type check, ESLint, Prettier, licences and sources
mise run format             # format the JavaScript, TypeScript and CSS
mise run audit              # known vulnerabilities in the dependencies
mise run links              # the links between the Markdown files
mise run workflows          # the GitHub workflows, with actionlint and zizmor
mise run test               # all the test suites
```

CI runs the same checks on macOS and Linux, and they must pass before a pull
request can be merged.

- **Tests:** add a test that fails without your fix.
- **Docs:** if your change affects what users see, update the relevant page
  in `docs/`. The pages in `docs/reference/` are generated, so change their
  source and run `mise run docs:generate`.
- **SDK:** every review in a person's history renders with the SDK the app
  serves at `/sdk/v1/`. Treat the public JavaScript API of
  `pinrail-plugin.js` and the classes in `pinrail-plugin.css` as a
  compatibility surface: add to them, but do not rename, remove or change
  what exists.
- **Commits:** keep each commit to one change. Write the title as a sentence
  that says what the commit does, without an `area:` prefix. For example:
  *Let the artifact plugin take its page as an HTML file*.

## License

Pinrail is licensed under the [Apache License 2.0](LICENSE). By contributing,
you agree that your contributions are licensed under the same terms.
