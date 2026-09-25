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
pinrail submit <plugin> --title "<title>" --data payload.json --wait --format markdown
```

It prints the review's id at once, `review <id>: …`, then waits for the
person's decision and prints it; keep the id for `--revises` and
`pinrail wait <id>`. A payload the plugin does not accept fails at once
(exit 2) with what is wrong: fix it and submit again. With the decision:

- Carry on with your task, doing what the person approved, as they
  decided it.
- If they asked for changes, make them and submit the new version with
  `--revises <id>`, so they see it beside the last one.
- Exit 5: they discarded it. Stop the work it was about, and don't ask
  again.

## Discovering plugins

Your instructions or skill usually name the plugin for a task. Only when
they don't, or you don't know what is installed:

```sh
pinrail plugins describe             # every plugin, a line each: when to use it
pinrail plugins describe <plugin>    # its payload schema and an example
```

Two come with every install:

- `list`: items (findings, proposed changes, tasks) the person accepts or
  rejects one by one.
- `feedback`: questions or choices only the person can answer, before you
  go on.

## Rules

- Give a title the person will recognise in an inbox of reviews.
- Say which project it is for. Inside a git checkout the command fills in
  the repository and branch; elsewhere add `--origin repo=<project name>`.
- Every command explains itself: `pinrail <command> --help`.
