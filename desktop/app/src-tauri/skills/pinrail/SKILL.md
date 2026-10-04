---
name: pinrail
description: Submit a review to the person through Pinrail and wait for their decision. Use when a prompt, a skill or your instructions tell you to ask, check with, or get approval from the person through Pinrail.
metadata:
  managed-by: Pinrail
  version: {version}
---

# Pinrail

Pinrail is an app on this computer where a person reviews what an agent
proposes and decides. You submit a review with the `pinrail` command,
which waits until the person decides in the app and then prints their
decision.

This skill explains how to submit. The prompt, skill or instructions that
send you here say when to ask, and often which plugin to use.

## Submitting

```sh
pinrail submit <plugin> --title "<title>" --data payload.json --wait
```

If your shell does not find `pinrail`, stop and ask the person to
install the Pinrail command line from the Pinrail app, in Settings ›
Data. Do not look for it elsewhere or install it yourself.

- A plugin is one kind of review, with its own payload. Use the plugin you
  were told to use. When none was named, `pinrail plugins` lists the
  installed ones and says when to use each.
- `pinrail plugins describe <plugin>` prints the plugin's payload schema
  and an example. Build the payload from the schema.
- The command prints the review's id at once, then the decision when the
  person makes it. Keep the id.

## The decision

- Carry on with your task as the person decided.
- When they ask for changes, make them and submit the new version with
  `--revises <id>`, so they see it beside the previous one.
- Exit code 5 means they discarded the review. Stop the work it was about,
  and do not ask again.
- Exit code 2 means the payload does not fit the plugin. The output says
  what is wrong. Fix it and submit again.
- If your commands stop after a time limit, add `--timeout <seconds>`
  below that limit. Exit code 4 means the review is still waiting: run
  `pinrail wait <id> --timeout <seconds>` until it ends.

## Rules

- Give a title the person will recognise in their inbox.
- Never decide or discard a review you submitted. The person does.
- Every command explains itself: `pinrail <command> --help`.

## When something does not work

Run `pinrail docs`. It is the guide that matches the installed version,
with topics on attaching files, revising a review, and every exit code.
