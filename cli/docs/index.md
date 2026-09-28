---
title: Pinrail
summary: What Pinrail is, the loop, and the rules to keep.
menu: [asking, plugins, instructions]
---
# Pinrail, for agents

Pinrail puts what you propose in front of a person, who decides in the
Pinrail app, and hands you their answer. Ask at the moments your
instructions name, not in chat, and don't go ahead without the answer.

## The loop

```sh
pinrail submit <plugin> --title "<title>" --data payload.json --wait
```

It prints the review's id at once, then waits for the person's decision
and prints it. Keep the id for `--revises` and `pinrail wait <id>`. If your
commands are stopped after a time limit, give every `submit --wait` and
`wait` a `--timeout <seconds>` below that limit. When one exits with 4
because the time ran out, run `pinrail wait <id> --timeout <seconds>` again
until the review ends. A payload the plugin does not accept fails at once
(exit 2) with what is wrong: fix it and submit again. With the decision:

- Carry on with your task, doing what the person approved, as they
  decided it.
- If they asked for changes, make them and submit the new version with
  `--revises <id>`, so they see it beside the last one.
- Exit 5: they discarded it. Stop the work it was about, and don't ask
  again.

## Discovering plugins

Your instructions usually name the plugin for a task. When they don't,
see what is installed:

```sh
pinrail plugins                      # every plugin, a line each: when to use it
pinrail plugins describe <plugin>    # its payload schema and an example
```

Two come with every install:

- `list`: items (findings, proposed changes, tasks) the person accepts or
  rejects one by one.
- `feedback`: questions or choices only the person can answer, before you
  go on.

## Rules

- Give a title the person will recognise in their inbox.
- Say which project it is for. Inside a git checkout the command fills in
  the repository and branch; elsewhere add `--origin repo=<project name>`.
- Every command explains itself: `pinrail <command> --help`.
- Output is markdown. Add `--json` only to process the result by program.
- Never decide or discard a review you submitted. The person does.
