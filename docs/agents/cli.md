---
title: The CLI
description: "The pinrail command, in use: submitting a review, waiting for the decision, and everything around it."
---

`pinrail` is the command an agent uses to ask a person before it acts. It sends a review to the Pinrail app on your machine, waits while you decide, and prints the decision: as markdown for an agent to read, or as JSON for a script to branch on.

```sh
pinrail submit review --title "Dedup tickets on save — round 1" \
  --origin repo=acme/api,workflow=review,ref=42 \
  --data proposals.json --wait
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

## An agent teaches itself

Pinrail doesn't need you to explain it to your agent. The command explains itself, and it does so a step at a time: each answer is short, and ends with where to look next. An agent reads only what its task needs, so its context stays small and nothing is out of date, because the text comes from the Pinrail you have installed.

```sh
pinrail docs                          # what Pinrail is, the loop, the rules, and a menu
pinrail docs plugins/building         # one brief, and the briefs under it
pinrail docs --tree                   # the whole map
```

`pinrail docs` starts with a screen: what Pinrail is, the one command an agent runs to ask, what to do with the answer, and the rules that apply every time. Its menu leads to short briefs, written for an agent at work rather than a person reading: asking and its exit codes, finding a plugin, writing a standing rule for itself, and building a plugin, down to the view's contract, the design language and the manifest's schema. `pinrail --help` points there, and so does everything else an agent meets first: the `describe` index, a new plugin's `AGENTS.md`, the prompts in the app's setup.

The same idea runs through the rest of the command. `pinrail plugins describe` lists the plugins a line each before any of them in full; `pinrail plugins new` ends with the next commands to run; `submit` says where the review is. An agent starts from `pinrail docs`, or from the plugin its instructions name, and finds the rest as it goes.

## Learning what to ask

Two steps tell an agent everything it needs to ask through Pinrail: first which plugin fits, then that plugin in full.

```sh
pinrail plugins describe                          # every usable plugin, a line each
pinrail plugins describe review # one plugin, in full
```

Without a name, `describe` is an index: each plugin in a line with **when to use it**, in the plugin author's words, and the files it takes. After the plugins come the command to submit with, what the finished review carries, and what every [exit code](#exit-codes) means.

With a name, it describes that plugin in full:

- what the plugin is for, and when to use it;
- the **payload schema**, and an **example payload** that passes it;
- the **decision schema**: the shape of `decision.data` in the review that comes back.

`--all` describes every plugin in full at once, for a tool that wants it in one call. Like `submit`, `describe` starts the app if it is not running.

:::tip[Point the agent at it]
An agent that runs `pinrail plugins describe` can choose a plugin, read that plugin in full, and write its payload without a person spelling any of it out.
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
| `--json` | Print the review as JSON instead of markdown, for a script. |
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
pinrail submit --request request.json --wait
```

The file takes the same keys as the flags: `plugin`, `title`, `payload`, `origin`, `summary`, `revises`, `expires_at`, `requested_by`, and `attachments`, a map of name to path relative to the file. Any flag given as well overrides the file's key, and `--data` replaces its payload. So a new round is the same file with one more flag:

```sh
pinrail submit --request request.json --revises <id> --wait
```

### A review in a browser

`submit` says where the new review is: in the app, and in a browser.

```text
review r_…: open it in Pinrail (pinrail://reviews/r_…) or preview it in a browser: http://127.0.0.1:4747/preview/reviews/r_…
```

The preview is the review as the app shows it: the plugin's view, fed the review, with the hand-over button. An agent with a browser tool opens it to see what it asked, and to try a plugin it is building. Handing over there checks the decision against the plugin's schema and decides nothing: a decision that passes is shown as the agent would get it, and one that fails comes back to the view. Deciding is the person's, in the app.

### A plugin's sample

A plugin can ship a sample review. Send it to see what the plugin looks like, or to try Pinrail end to end, without writing a payload:

```sh
pinrail submit list --sample --wait
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
  --attach out/pivot.glb --attach out/v2.glb=column.glb --wait
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
id=$(pinrail submit list --title "Nightly cleanup" --data items.json --json | jq -r .id)
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

Markdown is the default, because an agent reads it: the title, where the review came from, who decided and when, a tally, your note, then the decision, each plugin rendering its decision in a way that suits it. Every other command prints markdown too: a listing a line per review, a plugin's description, what a plugin command did.

JSON is for a script or a tool that processes the result rather than reads it: the whole review with the decision attached, to read with `jq` or anything else.

| Set | Effect |
|---|---|
| `--json` | JSON instead of markdown. |
| `PINRAIL_JSON=1` | JSON for every command in this environment, such as a CI job. |
| `--pretty` | Indented JSON. |

`--decision-out` always writes JSON.

## Every command

| Command | What it does |
|---|---|
| `pinrail submit <plugin>` | Submit a review. |
| `pinrail wait <id>` | Block until a review leaves pending, then print it. |
| `pinrail show <id>` | Print a review with its payload and decision. |
| `pinrail list` | List reviews, newest first. Filter with `--status`, `--repo`, `--workflow`, `--ref`, `--plugin` and `--q`. |
| `pinrail rounds <id>` | Every round of a review, oldest first. |
| `pinrail events <id>` | A review's event log. |
| `pinrail open <id> [--browser]` | Open a review in the app, or with `--browser` its [preview](#a-review-in-a-browser) in the default browser. |
| `pinrail attachments list <id>` | The files a review carries. |
| `pinrail attachments get <id> <name>` | Save a file a review carries. `-o` says where; it never overwrites without `--force`. |
| `pinrail decide <id>` | Record a decision from a script. The app is the usual way. |
| `pinrail withdraw <id>` | Take a pending review back. Its waiter exits 3. |
| `pinrail discard <id>` | Discard a pending review. Its waiter exits 5. |
| `pinrail export <dir>` | Write every review as JSON files under a directory. |
| `pinrail serve` | Start the app's server if it is not running, and print its URL. |
| `pinrail plugins` | List installed plugins, as a table or with `--json` as data, and [install, update or remove](/docs/using/installing-plugins/) them. |
| `pinrail plugins new <name> [--link]` | A new plugin that needs no build or npm: manifest, schemas, a sample, a view with the SDK's types, and an `AGENTS.md`. `--link` installs it right away. See [Writing a plugin](/docs/building/writing/#create-the-folder). |
| `pinrail plugins check [dir]` | What the app would make of a plugin folder, installing nothing: why it would refuse it, and each feature it would drop. Exits 0 when it would take it, 2 when not. |
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
| `PINRAIL_JSON` | `1` for JSON output from every command, as `--json` gives. |
| `PINRAIL_REQUESTED_BY` | Who is asking, shown on every review. |
