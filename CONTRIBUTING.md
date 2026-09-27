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
