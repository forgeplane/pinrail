---
name: pinrail
description: Submit a review to the person through Pinrail and wait for their decision, or build a Pinrail plugin. Use when a prompt, a skill or your instructions tell you to ask, check with, or get approval from the person through Pinrail.
metadata:
  managed-by: Pinrail
  sha: {sha}
---

# Pinrail

Pinrail is an app on this computer where a person reviews what an agent
proposes and decides. You submit a review with the `pinrail` command,
which waits until the person decides in the app and then prints their
decision. Ask at the moments your instructions name, not in chat, and do
not go ahead without the answer.

If your shell does not find `pinrail`, stop and ask the person to install
the Pinrail command line from the Pinrail app, in Settings › Data. Do not
look for it elsewhere or install it yourself.

## Submitting

```sh
pinrail submit <plugin> --title "<title>" --data payload.json --wait
```

- A plugin is one kind of review, with its own payload. Use the plugin you
  were told to use, or choose one as [choosing a plugin](#choosing-a-plugin)
  says.
- `pinrail plugins describe <plugin>` prints the plugin's payload schema
  and an example. Build the payload from the schema, starting from the
  example.
- The command prints the review's id at once, then the decision when the
  person makes it. Keep the id.

The options of `submit`:

- `--title`: what the person sees in the inbox. Required. Give a title
  the person will recognise.
- `--data <json>`: the payload, inline, in a file, or `-` for stdin.
- `--origin repo=…,ref=…,url=…`: the project, the branch or pull request,
  and a link back. Inside a git checkout the command fills in `repo` and
  `ref`. Outside one, give at least `--origin repo=<project name>`.
- `--wait`: block until the review ends, then print it.
- `--timeout <seconds>`: with `--wait`, stop waiting after this long. If
  your commands stop after a time limit, keep it below that limit.
  `PINRAIL_TIMEOUT` sets it once for every wait.
- `--json`: the whole review as JSON. The person's answer is its
  `decision.data`, shaped by the plugin's decision schema. Use it only to
  process the result by program, and read the default Markdown otherwise.
- `--dry-run`: run every check without creating a review. It exits with 0
  if the review would be accepted, and with 2 if not.
- `--revises <id>`: the new version of a review, shown beside the last one.

stdout carries the result. stderr carries errors, warnings, and a line
that starts with `pinrail: review <id> submitted` as soon as the review
exists, so you have the id while `--wait` blocks.

## The decision and the exit codes

- `0`: decided. Carry on with your task, doing what the person approved,
  as they decided it. Treat anything they left undecided as not approved.
- `1`: bad arguments, the request could not be sent, or the app failed.
  Report the error.
- `2`: the app refused the request, for example a payload that does not
  fit the plugin's schema, or a plugin that is not installed. The output
  says what is wrong. Fix it and submit again.
- `3`: the review was withdrawn, or it expired before anyone decided.
  Stop, and say that nobody decided.
- `4`: the review is still waiting, and `--timeout` ran out. Run
  `pinrail wait <id> --timeout <seconds>` until it ends.
- `5`: the person discarded the review. Stop the work it was about, report
  their reason, and do not ask again.

When the person asks for changes, make them, and submit the whole new
version with `--revises <id>`, with the same plugin. A round still waiting
when you revise it is withdrawn. `pinrail rounds <id>` prints every round.

## Files

Some plugins take files beside the payload: `pinrail plugins` says "Takes
files" and which kinds. The plugin's payload schema marks where each one
goes, as `{ "$attachment": "<name>" }`. Put that in the payload, and send
each file it names:

```sh
pinrail submit <plugin> --title "<title>" --data payload.json --attach render.png --attach out/v2.glb=model.glb --wait
```

`--attach PATH` sends a file under its own name, and `--attach PATH=NAME`
under the name the payload uses.

## Several questions

If the plugin takes several questions or items in one review, send them
together. Otherwise submit each without `--wait`, then run
`pinrail wait <id>` on each. Every wait exits with its own code.

`pinrail show <id>` prints where a review stands. `pinrail withdraw <id>
--reason "<why>"` withdraws one you no longer need. `pinrail list` prints
what is waiting on the person in this project.

## Choosing a plugin

```sh
pinrail plugins                                    # every plugin, a line each: when to use it
pinrail plugins describe <plugin>                  # its payload schema and an example
pinrail plugins describe <plugin> --decision-schema   # what it returns
```

A plugin fits only if it both shows what the person needs to review and
returns the decision you need: a diff view does not make a plugin fit for
approving a commit if it does not return an approval. Do not bend the task
to fit a schema, or make up an item just to have something to decide on.
Most people have `list`, for items to accept or reject one by one, and
`feedback`, for questions only the person can answer.

If no plugin returns the decision you need, build one for the task, as
[building a plugin](references/building.md) describes. Installing someone
else's plugin, upgrading it or removing it is the person's call: do it only
when they ask.

## Asking at the right moment

When the person wants you to ask before something, write it down where
your standing instructions live, so you and other sessions keep doing it:
`CLAUDE.md` or a skill for Claude Code, `GEMINI.md` for Gemini CLI, and
`AGENTS.md` or a skill for the others. Name the moment precisely, such as
"before posting review comments", and write one rule per moment:

```md
## Ask before <the moment>

Before you <the moment>, ask me through Pinrail and wait for my decision.
Don't ask in chat and don't go ahead without an answer.

1. Write <what you propose> as a payload for the `<plugin>` plugin:
   <its shape, as `pinrail plugins describe <plugin>` gives it>.
2. pinrail submit <plugin> --title "<a title I'll recognise>" \
     --data <file>.json --wait
3. Act only on what I decided; treat anything undecided as not approved.
4. If I ask for changes, send the new version with `--revises <id>`.
5. Exit 5 means I said stop: drop the work and don't ask again.
```

Run `pinrail plugins describe <plugin>` once while you write the rule, and
put the payload's shape in the rule itself.

## Rules

- Never decide or discard a review you submitted. The person does.
- Every command explains itself: `pinrail <command> --help`.

## More

- [Building a plugin](references/building.md): Make a plugin: create it, design its decision, work on its view with the person, test it, and try it in the app.
