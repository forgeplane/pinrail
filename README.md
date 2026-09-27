# Pinrail

Pinrail is a desktop app that puts a person in the loop of an agent's work.
Before an agent does something that matters, like posting review comments,
sending emails or shipping a page, it asks through Pinrail and waits. You see
the review in the app, shown in a view made for its content. You decide, and
the agent continues with your decision.

Two ideas shape it. **The agent decides when to ask**: its own instructions
say which steps need a person and what to send, so Pinrail fits any agent that
can run a command. **You decide what asking looks like**: every kind of
review is a plugin, and anyone can write one for their own work.

Status: early development.

## How it works

You tell the agent when to stop for you, in whatever instructions it follows:
a skill, a project's agent file, a prompt. "Before posting review comments,
submit them to Pinrail as a `review` and wait." When it reaches that step, the
agent calls the `pinrail` CLI with a review: a plugin name, a title and a JSON
payload. The app shows it in its inbox, notifies you, and renders it with
that plugin's view: a diff with proposed comments, a set of draft emails, an
HTML page to comment on element by element. You accept, reject, edit or
comment, then hand the decision over. The CLI returns it to the agent.

```sh
pinrail submit review --title "Dedup tickets on save" \
  --origin repo=acme/api,workflow=pr-review,ref=42 \
  --data proposals.json --wait
```

The command blocks until you decide, then prints the decision as markdown
for the agent to act on (or JSON for a script), and exits with a code that
says how the review ended: decided, discarded with an instruction to stop,
withdrawn or expired, or still pending when its `--timeout` ran out.

When you ask for changes, the agent submits a new round that revises the
last one, and the app shows your previous verdicts beside it. Every round
and decision is kept, so history can be reopened and rendered again.

## The app

- **Inbox and history.** What is waiting, grouped by project, and everything
  decided before. The sidebar keeps the oldest waiting reviews one click away
  on every page.
- **Notifications and the menu bar.** A new review raises a notification;
  the menu bar shows the count. The window can close while the app keeps
  listening.
- **Local by design.** The app runs its server on loopback, and reviews and
  decisions stay on your machine.

## Plugins

A plugin defines one kind of review: the payload an agent sends, the
decision you give back, and the view you decide in. Two plugins are built
into the app, `list` and `feedback`. The other official plugins are optional,
and you install the ones you need with
`pinrail plugins install github.com/forgeplane/pinrail/plugins/<name>`.
Anyone can write a plugin for what their agents do, such as triaging alerts,
approving a deploy or choosing between designs, and share it for others to
install.

| Plugin | For |
|---|---|
| `list` (built in) | items grouped under headings, each accepted or rejected with a note |
| `feedback` (built in) | questions answered in one pass: choices, yes or no, and free text |
| [`review`](plugins/review/README.md) | a code review: the diff and the agent's proposed comments |
| [`email`](plugins/email/README.md) | draft emails to edit, send, revise or discard |
| [`artifact`](plugins/artifact/README.md) | an HTML page to comment on, element by element |
| [`calendar`](plugins/calendar/README.md) | times to arrange around a calendar, one suggested slot picked per item |
| [`logo`](plugins/logo/README.md) | candidate logo marks and icons, seen at every size, with a favourite picked |
| [`model`](plugins/model/README.md) | candidate 3D models to orbit under studio light, with changes asked for on their parts |

A plugin is a manifest, two JSON schemas and an HTML view.
`pinrail plugins new <name>` creates one. The
[`pinrail-plugin`](pinrail-plugin/README.md) SDK runs a plugin in a browser
without the app, tests it, and creates plugins whose views are built with a
framework. To share a plugin, publish its repository or a GitHub release. The
app installs a plugin from either, or from a folder, and serves a linked
folder directly while you work on it.

## Repository

| Directory | Contents |
|---|---|
| [`desktop/`](desktop/) | the app: a Rust core (API, storage, plugins), a Tauri shell and a React UI |
| [`cli/`](cli/README.md) | the `pinrail` CLI agents call |
| [`plugins/`](plugins/README.md) | the official plugins |
| [`pinrail-plugin/`](pinrail-plugin/README.md) | the plugin SDK, development shell and test harness |
| [`e2e/`](e2e/README.md) | end-to-end tests: the CLI and the app's UI against the headless core |
| [`docs/`](docs/) | the documentation, published on the website |
| [`website/`](website/) | the website |

## Development

Tool versions are pinned in `mise.toml`, and `mise install` sets them up.

```sh
mise run dev:desktop        # the app with live reload
mise run test               # every test suite
mise run lint               # rustfmt, clippy, the type check and the licences, as CI runs them
```

[CONTRIBUTING.md](CONTRIBUTING.md) describes each test suite and how to run
it. Read it before opening a pull request. To
report a vulnerability, follow [SECURITY.md](SECURITY.md).

## License

Pinrail is licensed under the [Apache License 2.0](LICENSE). See
[`NOTICE`](NOTICE) for the copyright and the third-party notices.
