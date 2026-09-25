---
title: The CLI
description: "The pinrail command, in use: submitting a review, waiting for the decision, and everything around it."
---

`pinrail` is the command an agent uses to ask a person before it acts. It sends a review to the Pinrail app on your machine, waits while you decide, and prints the decision: as markdown for an agent to read, or as JSON for a script to branch on.

```sh
pinrail submit review --title "Dedup tickets on save — round 1" \
  --origin repo=acme/api,workflow=review,ref=42 \
  --data proposals.json --wait --format markdown
```

```md title="What the agent reads"
# Dedup tickets on save — round 1

review · acme/api · review · 42
Decided by alice at 2026-09-10 09:00 · 1 rejected

## Proposals

- **#18 rejected** `lib/acme/tickets.ex:149` — reversing twice is a no-op with a cost (major)
  > dont nitpick

Undecided: #19, #20
```

The CLI holds no state and makes no decisions of its own. Output goes to stdout, diagnostics to stderr, and every outcome has an exit code, so it is safe to call from any shell, CI job or agent harness.

## Learning what to ask

One command tells an agent everything it needs to ask through Pinrail:

```sh
pinrail plugins describe                          # every usable plugin, as JSON
pinrail plugins describe review --format markdown # one plugin, as a document
```

For each plugin you get:

- what the plugin is for, and **when to use it**, in the plugin author's words;
- the **payload schema**, and an **example payload** that passes it;
- the **decision schema**: the shape of `decision.data` in the review that comes back.

After the plugins come the command to submit with, what the finished review carries, and what every [exit code](#exit-codes) means. Like `submit`, it starts the app if it is not running.

:::tip[Point the agent at it]
An agent that runs `pinrail plugins describe --format markdown` at the start of a session can choose a plugin and write its payload without a person spelling either out.
:::

## Submitting a review

```sh
pinrail submit <plugin> --title <title> --data <file> [--wait]
```

| Flag | Meaning |
|---|---|
| `--title` | Required. What the review is about, as it will appear in your inbox. |
| `--data <file>` | The payload, as JSON. `--data -` reads it from stdin. |
| `--attach <path>[=<name>]` | A file to send beside the payload. Only a plugin that declares files accepts them. Repeat for more. See [Sending files](#sending-files). |
| `--wait` | Block until the review is decided or ends, then print it. Without it, `submit` prints the new review and returns at once. |
| `--dry-run` | Run every check a submission gets and create no review. Exits 0 when it would be accepted, 2 with the violations. |
| `--request <file>` | The whole request as one JSON file. See [The whole request in one file](#the-whole-request-in-one-file). |
| `--sample` | Send the plugin's sample, a review it ships to show what it looks like, in place of a payload. `--title` and `--origin` still apply. See [A plugin's sample](#a-plugins-sample). |
| `--format markdown` | Print the decision as markdown instead of JSON. |
| `--origin` | Where the review comes from: `repo=…,workflow=…,run_id=…,ref=…,url=…`. The app groups reviews by project and links back to `url`. Inside a git checkout, `repo` and `ref` default to the remote's `owner/name` and the current branch; outside one, give `repo` a short name for the project. |
| `--revises <id>` | This review is a new round of an earlier one. |
| `--timeout <seconds>` | With `--wait`: give up after this long, exit 4, and leave the review pending. |
| `--decision-out <file>` | Also write the decision's data, as JSON, to a file. |
| `--summary` | The counts the inbox shows beside the title. |
| `--expires-at` | Close the review if nobody decides by then. |
| `--requested-by` | Who is asking, shown on the review. Defaults to `PINRAIL_REQUESTED_BY`, then `pinrail-cli`. |

`create` is an alias for `submit`.

### The whole request in one file

Instead of flags, the agent can write the whole request as one JSON file and pass it with `--request` (or `--request -` to read stdin):

```json title="request.json"
{
  "plugin": "list",
  "title": "Sentry triage",
  "origin": { "repo": "acme/api", "ref": "main" },
  "payload": { "groups": [ … ] }
}
```

```sh
pinrail submit --request request.json --wait --format markdown
```

The file takes the same keys as the flags: `plugin`, `title`, `payload`, `origin`, `summary`, `revises`, `expires_at`, `requested_by`, and `attachments`, a map of name to path relative to the file. Any flag given as well overrides the file's key, and `--data` replaces its payload. So a new round is the same file with one more flag:

```sh
pinrail submit --request request.json --revises <id> --wait --format markdown
```

### A plugin's sample

A plugin can ship a sample review. Send it to see what the plugin looks like, or to try Pinrail end to end, without writing a payload:

```sh
pinrail submit list --sample --wait --format markdown
```

The review arrives like any other; decide it and the command prints your decision, as an agent would read it. `pinrail plugins describe <plugin>` says `"sample": true` for a plugin that has one, and a plugin without one is an error that says so. The same sample is a button on the plugin's row in *Settings › Plugins*.

### Checking a payload first

```sh
pinrail submit review --title "Dedup tickets on save" --data review.json --dry-run
```

A dry run checks the title, the origin and the payload against the plugin's schema, exactly as a submission would, and nothing reaches the inbox. When something is wrong it exits 2 and prints each violation with a JSON pointer to it, such as `/payload/proposals/0/line`, so the agent can fix the payload before a person sees it.

### Sending files

Some plugins take files beside the payload. The plugin's payload schema says where each file goes, and `pinrail plugins describe` shows it with the kinds and sizes the plugin accepts; a plugin that declares none refuses a submission with files. Build the payload as the schema says, and send each file it names with `--attach`. A schema marks a file's place with an object whose one key, `$attachment`, holds the file's name. For the [3D model](/docs/plugins/model/) plugin:

```sh
pinrail submit model --title "Halden desk lamp — round 1" --data models.json \
  --attach out/pivot.glb --attach out/v2.glb=column.glb --wait --format markdown
```

```json title="models.json"
{
  "models": [
    {
      "id": "L1",
      "name": "Pivot",
      "file": { "$attachment": "pivot.glb" }
    }
  ]
}
```

A file keeps its own name unless another follows `=`. The CLI checks the whole submission before it uploads anything, so a refused one moves nothing. It then uploads only the files the app does not have yet, which makes a new round cheap: only the files that changed are sent again. A file may be up to 100 MB, and a review may carry 32, unless the plugin sets lower limits.

The files come back with `pinrail attachments`:

```sh
pinrail attachments list <id>                        # name, size, media type and hash of each
pinrail attachments get <id> pivot.glb -o pivot.glb  # save one; -o - writes it to stdout
```

## Waiting

`submit --wait` is the usual way: one command that submits, waits and prints. When the waiting has to happen somewhere else, submit without `--wait` and wait later, from any process:

```sh
id=$(pinrail submit list --title "Nightly cleanup" --data items.json | jq -r .id)
# …later, or elsewhere
pinrail wait "$id"
```

Waiting survives the app restarting. `wait` polls the app and retries when the connection drops, so a restart costs a few seconds, not the review.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Done. For `wait` and `submit --wait`: the review was decided. |
| `1` | Error: bad arguments, the app unreachable, a file that could not be read or written. |
| `2` | The app refused the request, for example a payload the plugin's schema rejects. The details are on stderr. |
| `3` | The review was withdrawn by the agent, or expired, before anyone decided. |
| `4` | `--timeout` ran out. The review is still pending. |
| `5` | The person discarded the review: stop the work it was gating. |

:::caution[Exit 5 is "no, and stop"]
A discarded review has no decision, so `--decision-out` writes nothing. The printed review says who discarded it and why. Stop the work, report the reason, and do not retry or submit a new round.
:::

## Output

JSON is the default. It is the whole review, with the decision attached, for a script to read with `jq` or anything else. Markdown is for agents: the title, where the review came from, who decided and when, a tally, your note, then the decision. Each plugin renders its decision in a way that suits it.

| Set | Effect |
|---|---|
| `--format json` | The default. |
| `--format markdown` | The decision as prose. |
| `PINRAIL_FORMAT=markdown` | Make markdown the default in this environment. |
| `--pretty` | Indented JSON. |

`--decision-out` always writes JSON, whatever `--format` says.

## Every command

| Command | What it does |
|---|---|
| `pinrail submit <plugin>` | Submit a review. |
| `pinrail wait <id>` | Block until a review leaves pending, then print it. |
| `pinrail show <id>` | Print a review with its payload and decision. |
| `pinrail list` | List reviews, newest first. Filter with `--status`, `--repo`, `--workflow`, `--ref`, `--plugin` and `--q`. |
| `pinrail rounds <id>` | Every round of a review, oldest first. |
| `pinrail events <id>` | A review's event log. |
| `pinrail open <id>` | Open a review in the app. |
| `pinrail attachments list <id>` | The files a review carries. |
| `pinrail attachments get <id> <name>` | Save a file a review carries. `-o` says where; it never overwrites without `--force`. |
| `pinrail decide <id>` | Record a decision from a script. The app is the usual way. |
| `pinrail withdraw <id>` | Take a pending review back. Its waiter exits 3. |
| `pinrail discard <id>` | Discard a pending review. Its waiter exits 5. |
| `pinrail export <dir>` | Write every review as JSON files under a directory. |
| `pinrail serve` | Start the app's server if it is not running, and print its URL. |
| `pinrail plugins` | List installed plugins, and [install, update or remove](/docs/using/installing-plugins/) them. |
| `pinrail plugins describe [name]` | What an agent needs to ask with each plugin. See [Learning what to ask](#learning-what-to-ask). |

`pinrail <command> --help` lists every flag, and the [CLI reference](/docs/reference/cli/) has them all.

## Finding the app

The CLI talks to the server the Pinrail app runs on your machine, and finds it on its own, in this order:

1. `--url`, or `PINRAIL_URL` in the environment.
2. The `server.json` the running app writes into its data directory: `PINRAIL_DATA_DIR`, else `$XDG_DATA_HOME/pinrail`, else `~/.local/share/pinrail`.
3. `http://127.0.0.1:4747`, or the port in `PINRAIL_PORT`.

When nothing answers, `submit` and `serve` can start a server for you, if you say how with `PINRAIL_SERVER_CMD`. The app runs its server without a window with `--headless`:

```sh
export PINRAIL_SERVER_CMD='/Applications/Pinrail.app/Contents/MacOS/Pinrail --headless'
```

`submit --no-start` fails instead of starting one.

## Environment

| Variable | Used for |
|---|---|
| `PINRAIL_URL` | The server to talk to, ahead of anything the CLI finds. |
| `PINRAIL_DATA_DIR` | Where the running server's `server.json` and `server.log` are. |
| `PINRAIL_PORT` | The port to try when nothing is advertised. |
| `PINRAIL_SERVER_CMD` | How to start a server when none is running. |
| `PINRAIL_FORMAT` | `json` or `markdown`: the default for `--format`. |
| `PINRAIL_REQUESTED_BY` | Who is asking, shown on every review. |
