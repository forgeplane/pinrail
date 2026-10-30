---
title: The CLI
description: "The pinrail command, in use: asking questions, submitting a review, waiting for the decision, and everything around it."
---

`pinrail` is the command an agent uses to ask a person before it acts. It sends questions or a review to the Pinrail app on your machine, waits while you answer, and prints the answer: as markdown for an agent to read, or as JSON for a script to branch on.

```sh
pinrail submit code-review --title "Dedup tickets on save — round 1" \
  --origin repo=acme/api,workflow=review,ref=42 \
  --data proposals.json --wait
```

```md title="What the agent reads"
# Dedup tickets on save — round 1

review · acme/api · review · 42
Decided by alice at 2026-09-10 09:00 · 1 rejected

## Proposals

- **#18 rejected** `lib/acme/tickets.ex:149` — reversing twice is a no-op with a cost (major)
  > don't nitpick

Undecided: #19, #20
```

The CLI holds no state and makes no decisions of its own. The answer goes to stdout, errors and warnings to stderr, and every outcome has an exit code, so it is safe to call from any shell, CI job or agent harness.

## Built-in guidance for agents

You do not have to explain Pinrail to your agent. The `pinrail docs` command prints two guides, written for an agent at work: one on using Pinrail, and one on building a plugin, which an agent reads only when its task is to build one. The guides come from the Pinrail you have installed, so they always match it. They are the same files as the `pinrail` skill that [Settings › Agents](/docs/using/settings/#agents) adds to an agent, so an agent with the skill reads them as files.

```sh
pinrail docs                          # using Pinrail: asking, the answer, choosing a plugin
pinrail docs building                 # building a plugin
pinrail docs --tree                   # both, with what each covers
```

`pinrail docs` covers what Pinrail is, the command an agent runs to ask, its options and exit codes, what to do with the answer, files and revisions, choosing a plugin, and writing a standing rule for itself. `pinrail docs building` goes from a new plugin to a review in the person's inbox: the files, the decision, the view's contract, the dev shell, tests, the design language, settings and keys, and frameworks. `pinrail --help` points there, and so does a new plugin's `README.md`.

The other commands follow the same approach. `pinrail plugins` lists the plugins a line each before `describe` gives one in full; `pinrail plugins new` ends with the next commands to run; `submit` says where the review is. An agent starts from `pinrail docs`, or from the plugin its instructions name, and finds the rest as it goes.

## Learning what to ask

Two steps tell an agent everything it needs to ask through Pinrail: first which plugin fits, then that plugin in full.

```sh
pinrail plugins                       # every plugin, a line each
pinrail plugins describe code-review       # one plugin, in full
```

`pinrail plugins` lists each plugin in a line with what it is, **when to use it**, in the plugin author's words, and the files it takes, beside its version, where it comes from and whether it works.

`describe` gives one plugin in full:

- what the plugin is for, and when to use it;
- the **payload schema**, and an **example payload** that passes it;
- the **decision schema**, which describes `decision.data` in the review that comes back.

Some plugins also ship a skill, which says how to build their payload from the work. `pinrail plugins` says which ones, and `pinrail plugins skill <name>` prints it, with `pinrail plugins skill <name> <file>` for a reference it names. An agent you connected in *Settings › Agents* has the same skills in its `pinrail` skill already.

`--payload-schema`, `--example` and `--decision-schema` each print one of these parts alone, as JSON, for a tool or to start a payload with `--example > payload.json`. An agent that reads the decision as markdown does not need the decision schema. One that processes the decision with `--json` does, and the JSON output always includes it.

Like `submit`, `describe` can start a server when none is running, if `PINRAIL_SERVER_CMD` says how (see [Finding the app](#finding-the-app)). For a plugin that is installed but broken, it says what is wrong and how to check it.

:::tip[Point the agent at it]
An agent that runs `pinrail plugins`, then `pinrail plugins describe <name>`, can choose a plugin, read that plugin in full, and write its payload without a person spelling any of it out.
:::

## Asking questions

`pinrail ask` sends one or more short questions, waits for the answers and prints them. It needs no plugin and no payload file: each question is an argument, followed by its own options.

```sh
pinrail ask "Deploy to staging or production?" \
  --choice staging --choice production --recommend staging \
  --ask "Keep the old endpoint for a release?" --yes-no \
  --ask "What should the new endpoint be called?"
```

```text title="What the agent reads"
1. Deploy to staging or production?
staging
Note: production after the release notes are merged.

2. Keep the old endpoint for a release?
yes

3. What should the new endpoint be called?
Not answered.

Comment: Thanks for asking first.
```

| Option | Meaning |
|---|---|
| `<question>` | The first question, as one line of plain text, up to 300 characters. |
| `--ask <question>` | Another question. The options after it, up to the next `--ask`, apply to it. Up to 10 questions in all. |
| `--choice <text>` | An answer the person can pick, given once for each choice, at least twice. A choice can contain spaces and commas. Without it, the person types the answer. |
| `--multiple` | With `--choice`: the person can pick more than one. |
| `--yes-no` | A question answered with yes or no. |
| `--recommend <choice>` | The answer the agent would choose, shown as recommended. |
| `--context <markdown>` | More detail, shown below the question. `-` reads it from stdin, and `@<file>` from a file. |
| `--title <text>` | The title in the inbox. Without it, the title is the first question. |
| `--data <json>` | The questions as JSON, `{"questions": [...]}`, in place of the options above. |

![An agent session's thread: three questions waiting, the first answered with a note, and the box for a comment to the agent.](screenshot:ask-thread "The questions of one pinrail ask, waiting in the session's thread.")

The questions in one command are independent: the person answers them in any order, in the session's thread in the app, as [Questions and sessions](/docs/using/sessions/) describes. They can answer a question with choices in their own words, add a note to an answer, leave a question unanswered, and comment on the questions as a whole. With one question, the command prints only its answer. With `--json`, it prints the whole review: the answers are in `decision.data.answers`, and the comment in `agent_note`.

When a question depends on the answer to another, ask the first, then the next with another `pinrail ask`. For questions that depend on each other, or that need a long explanation each, the `feedback` plugin fits better.

`pinrail ask` always waits, and exits as `submit --wait` does: `5` when the person declines to answer. `pinrail show`, `wait`, `withdraw` and `list` work on questions as they do on reviews.

## Reporting progress

`pinrail status` says what the agent is doing, in one line, in its session's thread in the app. It does not wait and prints nothing.

```sh
pinrail status --start "Add rate limiting to the public API"
pinrail status "Running the end-to-end suite"
pinrail status --done "Pushed the fix to pinrail-ask"
```

The text is plain text of at most 200 characters, on one line. `--start` says the agent starts a task, and its text names the task: the newest task started is the session's title in the app, in the thread, on the Sessions page and in the sidebar. `--done` says the agent has finished, and its text is then optional. The app keeps every status, so the thread shows what the agent did between its questions and reviews, and the newest one shows under the session's title and on the Sessions page. A status after `--done` shows the agent working again, and a new `--start` gives the session the new task's title.

![A session's thread: statuses before and after an answered question, and the agent saying it has finished.](screenshot:session-status "Statuses around a question, up to pinrail status --done.")

Start each task with `--start`, and report each step that takes more than a minute or so. Use `--done` once the person has approved the work and the agent has committed or delivered it, or when the task ends with nothing to review, such as a search. Waiting for a review is a step, not the end. A status for every command would bury the questions and reviews that need the person.

The command finds the agent and its session as `submit` does, from the agent's environment, such as `CLAUDE_CODE_SESSION_ID`. With no session found, it exits with `1`. A text the app refuses, such as one with a line break, exits with `2`.

## Submitting a review

```sh
pinrail submit <plugin> --title <title> --data <file> [--wait]
```

| Flag | Meaning |
|---|---|
| `--title` | Required. What the review is about, as it will appear in your inbox. |
| `--data <json>` | The payload, as JSON: inline, such as `--data '{"groups": []}'`, in a file, or `-` for stdin. |
| `--attach <path>[=<name>]` | A file to send beside the payload. Only a plugin that declares files accepts them. Repeat for more. See [Sending files](#sending-files). |
| `--wait` | Block until the review is decided or ends, then print it. Without it, `submit` prints the new review and returns at once. |
| `--dry-run` | Run every check a submission gets and create no review. Exits 0 when it would be accepted, 2 with the violations. |
| `--request <json>` | The whole request as JSON, inline or in a file. See [The whole request in one file](#the-whole-request-in-one-file). |
| `--sample` | Send the plugin's sample, a review it ships to show what it looks like, in place of a payload. `--title` and `--origin` still apply. See [A plugin's sample](#a-plugins-sample). |
| `--json` | Print the review as JSON instead of markdown, for a script. |
| `--origin` | Where the review comes from: `repo=…,workflow=…,run_id=…,ref=…,url=…`. The app groups reviews by project and links back to `url`, which must be an `http` or `https` address. Inside a git checkout, `repo` and `ref` default to the remote's `owner/name` and the current branch; outside one, give `repo` a short name for the project. Any other key is dropped, with a warning on stderr. |
| `--revises <id>` | This review is a new round of an earlier one. It must revise the latest round, with the same plugin. A revised round that is still pending is withdrawn. |
| `--timeout <seconds>` | With `--wait`: give up after this long, exit 4, and leave the review pending. |
| `--decision-out <file>` | Also write the decision's data, as JSON, to a file. |
| `--expires-at` | Close the review if nobody decides by then. |
| `--requested-by` | Who is asking, shown on the review, with the agent's icon when the app knows it. Defaults to `PINRAIL_REQUESTED_BY`. Otherwise it is the coding agent the command runs under, as `AI_AGENT` or `AGENT` names it when either is set, or as found from the variables that `amp`, `antigravity`, `augment`, `claude-code`, `cline`, `codex`, `copilot`, `cursor`, `goose`, `grok`, `hermes-agent`, `kiro`, `opencode` and `pi` set. `AGENT` counts only when it names an agent Pinrail knows, which also includes `kimi`. Without any of these, it is `pinrail-cli`. Name the job or role with `--origin workflow=…`. |

Submitting the same review again while the first is still pending does not create a second one: the command answers the review already waiting, with its id. An agent that runs the command a second time, for example after its first attempt was stopped, waits on the same review, and the person decides it once.

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

The file takes the same keys as the flags: `plugin`, `title`, `payload`, `origin`, `revises`, `expires_at`, `requested_by`, and `attachments`, a map of name to path relative to the file. Any flag given as well overrides the file's key, and `--data` replaces its payload. So a new round is the same file with one more flag:

```sh
pinrail submit --request request.json --revises <id> --wait
```

### A plugin's sample

A plugin can ship a sample review. Send it to see what the plugin looks like, or to try Pinrail end to end, without writing a payload:

```sh
pinrail submit list --sample --wait
```

The review arrives like any other; decide it and the command prints your decision, as an agent would read it. `pinrail plugins describe <plugin>` says `"sample": true` for a plugin that has one, and a plugin without one is an error that says so. The same sample can be sent with **Send a sample** in the plugin's details in *Settings › Plugins*.

### Checking a payload first

```sh
pinrail submit code-review --title "Dedup tickets on save" --data review.json --dry-run
```

A dry run checks the title, the origin and the payload against the plugin's schema, exactly as a submission would, and nothing reaches the inbox. When something is wrong it exits 2 and prints each violation with a JSON pointer to it, such as `/payload/proposals/0/line`, so the agent can fix the payload before a person sees it.

### Sending files

Some plugins take files beside the payload. The plugin's payload schema says where each file goes, and `pinrail plugins describe <name>` shows it with the kinds and sizes the plugin accepts; a plugin that declares none refuses a submission with files. Build the payload as the schema says, and send each file it names with `--attach`. A schema marks a file's place with an object whose one key, `$attachment`, holds the file's name. For the [Image review](/docs/plugins/image/) plugin:

```sh
pinrail submit image --title "Empty inbox illustration — round 1" --data images.json \
  --attach out/paper-plane.png --attach out/v2.png=mailbox.png --wait
```

```json title="images.json"
{
  "images": [
    {
      "id": "A",
      "name": "Paper plane",
      "file": { "$attachment": "paper-plane.png" }
    }
  ]
}
```

A file keeps its own name unless another follows `=`. The CLI checks the whole submission before it uploads anything, so a refused one moves nothing. It then uploads only the files the app does not have yet, which makes a new round cheap: only the files that changed are sent again. A file may be up to 100 MB, and a review may carry 32 files adding up to 512 MB, unless the plugin sets lower limits.

The files come back with `pinrail attachments`:

```sh
pinrail attachments list <id>                        # name, size, media type and hash of each
pinrail attachments get <id> paper-plane.png -o paper-plane.png  # save one; -o - writes it to stdout
```

## Waiting

`submit --wait` is the usual way: one command that submits, waits and prints. When the waiting has to happen somewhere else, submit without `--wait` and wait later, from any process:

```sh
id=$(pinrail submit list --title "Nightly cleanup" --data items.json --json | jq -r .id)
# …later, or elsewhere
pinrail wait "$id"
```

Waiting survives the app restarting. `wait` polls the app and retries when the connection drops, so if the app restarts, the wait resumes a few seconds later and the review is not lost.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Done. For `wait` and `submit --wait`: the review was decided. |
| `1` | Error: bad arguments, the app could not be reached, a file could not be read or written, or the app failed on its side. `wait` and `submit --wait` keep waiting through an error inside the app until it answers again. |
| `2` | The app refused the request, for example a payload the plugin's schema rejects, or a folder that `plugins check` would not install. Why is on stderr. It lists each refused field with its reason, or, with `--json`, gives the app's JSON answer. |
| `3` | The review was withdrawn by the agent, or expired, before anyone decided. |
| `4` | `--timeout` ran out. The review is still pending. |
| `5` | The person discarded the review: stop the work it was gating, and do not ask again. |

:::caution[Exit code 5 means stop]
A discarded review has no decision, so `--decision-out` writes nothing. The printed review says who discarded it and why. Stop the work, report the reason, and do not retry or submit a new round.
:::

## Output

Markdown is the default, because an agent reads it: the title, where the review came from, who decided and when, a tally, your note, then the decision, each plugin rendering its decision in a way that suits it. Every other command prints markdown too: a listing a line per review, a plugin's description, what a plugin command did.

JSON is for a script or a tool that processes the result rather than reads it: the whole review with the decision attached, to read with `jq` or anything else.

| Set | Effect |
|---|---|
| `--json` | JSON instead of markdown. |
| `PINRAIL_JSON=1` | JSON for every command in this environment, such as a CI job. |
| `--pretty` | Indented JSON; with `--json`. |
| `--verbose`, `-v` | Also say on stderr what happened along the way: the origin read from git, the server started, the files uploaded. `PINRAIL_VERBOSE=1` sets it for every command. |

`--decision-out` always writes JSON.

## Every command

| Command | What it does |
|---|---|
| `pinrail ask <question>` | Ask the person one or more questions, wait for the answers and print them. See [Asking questions](#asking-questions). |
| `pinrail status <text>` | Say what the agent is doing in its session's thread: `--start` for a task, which names the session, and `--done` when it has finished. See [Reporting progress](#reporting-progress). |
| `pinrail submit <plugin>` | Submit a review. |
| `pinrail wait <id>` | Block until a review leaves pending, then print it. |
| `pinrail show <id>` | Print a review with its payload and decision. |
| `pinrail list` | The pending reviews of the project you are in, newest first; `--all` for every review. Filter with `--status`, `--repo`, `--workflow`, `--ref`, `--plugin` and `--q`. |
| `pinrail rounds <id>` | Every round of a review, oldest first. |
| `pinrail events <id>` | A review's event log. |
| `pinrail open <id> [--browser]` | Open a review in the app, or with `--browser` its [preview](/docs/building/writing/#see-it-in-a-browser), for building a plugin. |
| `pinrail attachments list <id>` | The files a review carries. |
| `pinrail attachments get <id> <name>` | Save a file a review carries. `-o` says where; it never overwrites without `--force`. |
| `pinrail decide <id>` | Record a decision from a script. The app is the usual way. |
| `pinrail withdraw <id>` | Take a pending review back. Its waiter exits 3. |
| `pinrail discard <id>` | Discard a pending review. Its waiter exits 5. |
| `pinrail export <dir>` | Write every review as JSON files under a directory. |
| `pinrail serve` | Start the app's server if it is not running, and print its URL. |
| `pinrail plugins` | List installed plugins, as a table or with `--json` as data, and [install or remove](/docs/using/installing-plugins/) them. |
| `pinrail plugins new <name> [--template plain\|typescript\|react] [--playwright] [--link]` | A new plugin: manifest, schemas, a sample, and a view with the SDK's types. The plain view needs no build or npm; `typescript` (or `ts`) and `react` write a view built by Vite. `--playwright` adds a first test, and `--link` installs a plain plugin right away. See [Writing a plugin](/docs/building/writing/#create-the-folder). |
| `pinrail plugins check [dir]` | What the app would make of a plugin folder, installing nothing: why it would refuse it, and each feature it would drop. It also checks the plugin's recorded decisions in `fixtures/` (see [Building with a framework](/docs/building/frameworks/)). Exits 0 when the app would take the plugin and every recorded decision checks out, 2 when not. Build a plugin that has a build step first, so that its view is there. It needs no running app. |
| `pinrail plugins schema [manifest\|attachment]` | Prints one of the plugin SDK's JSON Schemas: the manifest's, which every `manifest.json` is checked against, or the attachment schema, which a payload schema copies for a field that names a file. It needs no running app. |
| `pinrail plugins describe <name>` | What an agent needs to ask with a plugin; `--payload-schema`, `--example` or `--decision-schema` for one part alone. See [Learning what to ask](#learning-what-to-ask). |
| `pinrail docs [path]` | The briefs for an agent: how to ask, and how to build a plugin. `--tree` lists them all. |

`pinrail <command> --help` lists every flag, and the [CLI reference](/docs/reference/cli/) has them all.

## Finding the app

The CLI talks to the server the Pinrail app runs on your machine, and finds it on its own, in this order:

1. `--url`, or `PINRAIL_URL` in the environment.
2. The `server.json` the running app writes into its data directory: `PINRAIL_DATA_DIR`, else `$XDG_DATA_HOME/pinrail`, else `~/.local/share/pinrail`.
3. `http://127.0.0.1:4747`, or the port in `PINRAIL_PORT`.

When nothing answers, `submit`, `serve`, `plugins`, `plugins describe` and `plugins new --link` can start a server for you, if you say how with `PINRAIL_SERVER_CMD`. The CLI runs the command with `sh -c`, or with `cmd /C` on Windows. The app runs its server without a window with `--headless`:

```sh
export PINRAIL_SERVER_CMD='/Applications/Pinrail.app/Contents/MacOS/Pinrail --headless'
```

A server started this way uses the same data directory as the app, and only one of them can have it open at a time. While the headless server runs, opening the app shows which process to stop first.

`submit --no-start` fails instead of starting one.

## Environment

| Variable | Used for |
|---|---|
| `PINRAIL_URL` | The server to talk to, ahead of anything the CLI finds. |
| `PINRAIL_DATA_DIR` | Where the running server's `server.json` and `server.log` are. |
| `PINRAIL_PORT` | The port to try when nothing is advertised. |
| `PINRAIL_SERVER_CMD` | How to start a server when none is running. |
| `PINRAIL_TIMEOUT` | How many seconds `wait` and `submit --wait` wait, when `--timeout` is not given. Set it once below your command time limit. |
| `PINRAIL_JSON` | `1` for JSON output from every command, as `--json` gives. |
| `PINRAIL_VERBOSE` | `1` for the notes that `--verbose` prints, from every command. |
| `PINRAIL_REQUESTED_BY` | Who is asking, shown on every review, ahead of the agent the command finds itself running under. |
