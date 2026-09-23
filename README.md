# Pinrail

A pinrail is the small gate in a larger door: a person waits at it, looks at
what is being carried through, and lets it pass or not.

Pinrail is a desktop app that puts a person in the loop of an agent's work.
Before an agent does something that matters, like posting review comments,
sending emails or shipping a page, it asks through Pinrail and waits. You see
the request in the app, rendered for what it is, decide, and the agent carries
on with your decision.

Two ideas shape it. **The agent decides when to ask**: its own instructions
say which steps need a person and what to send, so Pinrail fits any agent that
can run a command. **You decide what asking looks like**: every kind of
request is a plugin, and anyone can write one specialised for their own work.

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
  --data proposals.json --wait --format markdown
```

The command blocks until you decide, then prints the decision as markdown
for the agent to act on (or JSON for a script), and exits with a code that
says how the review ended: decided, withdrawn, timed out, or discarded with
an instruction to stop.

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

A plugin defines one kind of request: the payload an agent sends, the
decision you give back, and the view you decide in. The plugins below are
samples bundled with Pinrail. Anyone can create their own for whatever their
agents do, like triaging alerts, approving a deploy, picking between designs
or answering an agent's questions, and share it for others to install.

| Plugin | For |
|---|---|
| `list` (built in) | items grouped under headings, each accepted or rejected with a note |
| [`review`](plugins/review/README.md) | a code review: the diff and the agent's proposed comments |
| [`email`](plugins/email/README.md) | draft emails to edit, send, revise or discard |
| [`artifact`](plugins/artifact/README.md) | an HTML page to comment on, element by element |
| [`calendar`](plugins/calendar/README.md) | times to arrange around a calendar, one suggested slot picked per item |
| [`logo`](plugins/logo/README.md) | candidate logo marks and icons, seen at every size, with a favourite picked |
| [`model`](plugins/model/README.md) | candidate 3D models to orbit under studio light, with changes asked for on their parts |

A plugin is a manifest, two JSON schemas and an HTML view. The
[`pinrail-plugin`](pinrail-plugin/README.md) package scaffolds one
(`pinrail-plugin create`), runs it in a browser without the app, and tests it.
To share a plugin, publish its repository or a GitHub release; the app
installs it from either, or from a folder, and serves a linked folder live
while you work on it.

## Repository

| Directory | Contents |
|---|---|
| [`desktop/`](desktop/) | the app: a Rust core (API, storage, plugins), a Tauri shell and a React UI |
| [`cli/`](cli/README.md) | the `pinrail` CLI agents call |
| [`plugins/`](plugins/README.md) | the official plugins and the plugin protocol |
| [`pinrail-plugin/`](pinrail-plugin/README.md) | the plugin SDK, dev shell and test harness |
| [`e2e/`](e2e/README.md) | end-to-end tests: the CLI and the app's UI against the headless core |

## Development

Tool versions are pinned in `mise.toml`; `mise install` sets them up.

```sh
mise run dev:desktop        # the app with live reload
mise run test:desktop       # the core's tests and the UI's type check
mise run e2e                # the CLI against the headless core
mise run e2e:shell          # the app's UI in a browser against the headless core
mise run test:plugins       # every plugin under the harness
mise run lint               # rustfmt, clippy and the type check, as CI runs them
```

## License

Pinrail is licensed under the [Apache License 2.0](LICENSE). See
[`NOTICE`](NOTICE) for the copyright and the third-party notices.
