# pinrail CLI

`pinrail` is the command-line tool that agents and scripts use to ask a person
through the Pinrail app. An agent submits a review, the person decides in the
app, and the command returns the decision: as Markdown for an agent to read,
or as JSON for a script.

```sh
pinrail submit review --title "Dedup tickets on save" --data proposals.json --wait
```

The CLI keeps no state of its own. It is a small Rust binary that sends HTTP
requests to the server the app runs on your computer. It writes its output to
stdout, errors and warnings to stderr, and ends with an exit code that says
how the review ended.

## Documentation

- [The `pinrail` command](../docs/agents/cli.md) is the guide to every
  command, its output and its exit codes.
- [Instructing an agent](../docs/agents/instructing.md) explains what to put
  in an agent's instructions.
- [Scripts and CI](../docs/agents/workflows.md) covers using the command from
  scripts.
- [The CLI reference](../docs/reference/cli.md) lists every command and
  option. It is generated from the code with `mise run docs:generate`.

Agents can also read the same guidance from the command itself with
`pinrail docs`, which prints the briefs kept in [`docs/`](docs/).

## Installing

The app installs the command. See
[Installing Pinrail](../docs/getting-started/install.md). To build it from a
checkout instead, with the Rust version pinned in the repository's
`mise.toml`:

```sh
cargo install --path cli
```

## Development

```sh
cargo build                 # target/debug/pinrail
cargo test                  # against a scripted HTTP server, with no app running
```

The end-to-end tests in [`e2e/`](../e2e/README.md) run the built CLI against
the app's server. [CONTRIBUTING.md](../CONTRIBUTING.md) describes every test
suite.

## License

Apache License 2.0. See [`LICENSE`](../LICENSE) and [`NOTICE`](../NOTICE).
