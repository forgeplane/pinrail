# wicket CLI

`wicket` is the command-line tool agents use to talk to the Wicket app: it is
how an agent asks a person before it acts. The agent describes what it is
about to do as a review, sends it to the app, and waits. The person opens the
review in the app, decides, and the command returns with the decision: as
JSON for a script to branch on, or as markdown for an agent to read in its
session.

```sh
wicket submit review --title "Dedup tickets on save — round 1" \
  --origin repo=acme/api,workflow=review,ref=42 \
  --data proposals.json --wait --format markdown
```

```markdown
# Dedup tickets on save — round 1

review · acme/api · review · 42
Decided by alice at 2026-09-10 09:00 · 1 rejected

## Proposals

- **#18 rejected** `lib/acme/tickets.ex:149` — reversing twice is a no-op with a cost (major)
  > dont nitpick

Undecided: #19, #20
```

The CLI holds no state and makes no decisions. It is a small, dependency-light
Rust binary that talks HTTP to the server the app runs on your machine, which
makes it safe to call from any shell, CI job or agent harness: output on
stdout, diagnostics on stderr, and an exit code for every outcome.

## Installing

Releases will ship the CLI inside the desktop app. Until the first release,
build it from a checkout with a Rust toolchain (the version is pinned in the
repository's `mise.toml`):

```sh
cargo install --path cli        # installs `wicket` into ~/.cargo/bin
```

The CLI needs the Wicket app, or its server, running to talk to. It finds it
on its own: see [Finding the server](#finding-the-server).

## A first review

Every review belongs to a plugin, which defines the payload an agent sends,
the decision a person gives back, and the view the app draws. The built-in
`list` plugin takes items grouped under headings and lets the person accept or
reject each one with a note. Save this as `triage.json`:

```json
{
  "intro": "Sentry triage for **acme-api**, last 7 days.",
  "groups": [
    {
      "title": "acme-api",
      "items": [
        {
          "id": 101,
          "severity": "blocker",
          "title": "Ecto.StaleEntryError in Tickets.close/1 (312×)",
          "body": "Two workers close the same ticket; the second update hits a stale row."
        }
      ]
    }
  ]
}
```

Then submit it and wait:

```sh
wicket submit list --title "Sentry triage" --data triage.json --wait
```

The command prints the review's URL to stderr and blocks. The app shows the
review in its inbox and raises a notification. When the person hands over
their decision, the command prints the review with the decision attached and
exits 0. The payload and decision shapes of the built-in plugin are in
[its README](../plugins/list/README.md); the other official
plugins, and how to write your own, are in [`plugins/`](../plugins/README.md).

## In an agent's session, and in a script

The two callers want different things from the same command, and the CLI
serves both.

An **agent** in a coding session reads the decision as text and acts on it.
Give it markdown: `--format markdown`, or `WICKET_FORMAT=markdown` set once
in the environment the agent runs in. The rendering leads with the title and
the outcome, then lists every item with its verdict and the person's note.
A plugin can ship its own template for this; the rest are rendered from the
shape of their decision.

A **script** or CI job branches on the result. It keeps the default JSON
and the exit code, and can write the decision's data to a file with
`--decision-out`, so a workflow that already reads a decisions file keeps
working unchanged:

```sh
if wicket submit list --title "MR !42" --origin repo=acme,workflow=review,ref=42 \
     --data payload.json --wait --decision-out mr-42.decisions.json > review.json; then
  ./apply-decisions mr-42.decisions.json
fi
```

`--decision-out` always writes JSON, whatever `--format` says.

## Waiting

`submit --wait` is the usual way in: one command that submits, waits and
prints. When the waiting has to happen elsewhere, `submit` without `--wait`
prints the new review and returns at once, and `wicket wait <id>` picks it
up later, from another process if need be.

Waiting survives the server going away. `wait` asks the server in short
polls and retries when a poll comes back empty or the connection drops, so a
restart of the app costs a few seconds and not the review. `--timeout
<seconds>` gives up after that long with exit 4 and leaves the review
pending, to be waited on again.

A review can end without a decision, and the exit code says how:

| Code | Meaning |
|---|---|
| 0 | Done. For `wait` and `submit --wait`: the review was decided. |
| 1 | Error: bad arguments, the server unreachable, a file that could not be read or written. |
| 2 | The server refused the request, for example a payload the plugin's schema rejects. Its JSON answer is on stderr. |
| 3 | The review was withdrawn by the agent, or expired, before anyone decided. |
| 4 | `--timeout` ran out. The review is still pending. |
| 5 | The person discarded the review: stop the work it was gating. |

Exit 5 deserves care. Discarding is the person's "no, and stop", made in the
app or with `wicket discard`. The printed review carries who discarded it and
why. An agent that gets exit 5 stops the work the review was about, reports
the reason, and neither retries nor submits a new round. There is no decision,
so `--decision-out` writes nothing.

## Rounds

When the person asks for changes, the agent makes them and submits again,
naming the review it answers with `--revises <id>`. The app shows the new
round with the previous round's verdicts beside it. `wicket list` shows only
the latest round of each review unless `--include-revised` is given, and
`wicket rounds <id>` prints every round of a review, oldest first.

## Commands

| Command | What it does |
|---|---|
| `wicket submit <plugin>` | Submit a review. `create` is an alias. |
| `wicket wait <id>` | Block until a review leaves pending, then print it. |
| `wicket show <id>` | Print a review with its payload and decision. |
| `wicket list` | List reviews, newest first, without payloads. |
| `wicket rounds <id>` | Every round of a review, oldest first. |
| `wicket events <id>` | A review's event log. |
| `wicket open <id>` | Open a review in the app. |
| `wicket decide <id>` | Record a decision from a script; the app is the usual way. |
| `wicket withdraw <id>` | Withdraw a pending review; its waiter exits 3. |
| `wicket discard <id>` | Discard a pending review as the person would; its waiter exits 5. |
| `wicket export <dir>` | Write every review as JSON files under a directory. |
| `wicket serve` | Start the server if it is not running, and print its URL. |
| `wicket plugins` | List the installed plugins, and manage them (below). |

`wicket <command> --help` lists every flag. The ones that matter most:

- **`submit`** takes `--title` (required), `--data <file>` or `--data -` for
  stdin, and `--origin repo=…,workflow=…,run_id=…,ref=…,url=…` to say where
  the review comes from; the app groups reviews by project and links back to
  the origin's URL. `--summary` sets the counts the inbox shows beside the
  title, `--expires-at` closes a review nobody decided in time, and
  `--requested-by` (or `WICKET_REQUESTED_BY`) names the caller.
- **`list`** filters with `--status` (comma-separated), `--repo` (`-` for
  reviews that name no project), `--workflow`, `--ref`, `--run-id`,
  `--plugin` and `--q`, whose words must all appear somewhere among the
  title, payload, plugin, requester, origin and who decided; it pages with
  `--limit` and `--cursor`, and `--all` follows the pages to the end.
- **`decide`** takes `--data` with the decision and `--note` for the agent.
  `withdraw` and `discard` take `--reason`.
- **Every command** accepts `--url`, `--pretty` and `--format`.

### Plugins

```sh
wicket plugins                                          # what is installed, and anything wrong with it
wicket plugins install ./my-plugin                      # copy a folder into the app's store
wicket plugins install ./my-plugin --link               # serve the folder live while you work on it
wicket plugins install github.com/acme/plugins/review@v3
wicket plugins install https://github.com/acme/wicket-review/releases
wicket plugins update [name]                            # reinstall from the source, when it has something new
wicket plugins remove <name>
```

An install shows its progress on stderr, including a build's output when the
plugin's manifest declares one. A plugin removed or moved to a new major
version keeps the copy that older reviews render from. The sources `install`
accepts are described in [`plugins/README.md`](../plugins/README.md#installing),
and [`wicket-plugin`](../wicket-plugin/README.md) scaffolds, runs and tests a
plugin of your own.

## Finding the server

The CLI looks for the server in this order, and uses the first it finds:

1. `--url`, or `WICKET_URL` in the environment.
2. The `server.json` the running server writes into its data directory:
   `WICKET_DATA_DIR`, else `$XDG_DATA_HOME/wicket`, else
   `~/.local/share/wicket`.
3. `http://127.0.0.1:4747`, or the port in `WICKET_PORT`.

When nothing answers, `submit` and `serve` start a server if you have told
the CLI how, through `WICKET_SERVER_CMD`: a shell command that runs one. The
desktop app's binary runs its server without a window with `--headless`,
which suits CI and remote machines:

```sh
# macOS
export WICKET_SERVER_CMD='/Applications/Wicket.app/Contents/MacOS/Wicket --headless'
# Linux, from the .deb or .rpm
export WICKET_SERVER_CMD='wicket-desktop --headless'
```

The server starts detached, in its own process group, and logs to
`server.log` in the data directory. `submit --no-start` fails instead of
starting one.

### Environment

| Variable | Used for |
|---|---|
| `WICKET_URL` | The server to talk to, ahead of anything the CLI finds. |
| `WICKET_DATA_DIR` | Where the running server's `server.json` and `server.log` are. |
| `WICKET_PORT` | The port to try when nothing is advertised. |
| `WICKET_SERVER_CMD` | How to start a server when none is running. |
| `WICKET_FORMAT` | `json` or `markdown`, the default for `--format`. |
| `WICKET_REQUESTED_BY` | Who is asking, shown on every review. Defaults to `wicket-cli`. |

## Development

```sh
cargo build                 # target/debug/wicket
cargo test                  # against a scripted HTTP server; no app needed
```

The end-to-end suite in [`e2e/`](../e2e/README.md) runs the built CLI against
the headless desktop server: submitting, waiting from two processes, a server
restart mid-wait, withdrawing, discarding, and installing plugins.

## License

Apache License 2.0; see [`LICENSE`](../LICENSE) and [`NOTICE`](../NOTICE).
