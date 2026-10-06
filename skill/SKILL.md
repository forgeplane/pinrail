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

## Submitting

```sh
pinrail submit <plugin> --title "<title>" --data payload.json --wait
```

If your shell does not find `pinrail`, stop and ask the person to install
the Pinrail command line from the Pinrail app, in Settings › Data. Do not
look for it elsewhere or install it yourself.

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
- Outside a git checkout, say which project the review is for with
  `--origin repo=<project name>`. Inside one, the command fills it in.
- Never decide or discard a review you submitted. The person does.
- Every command explains itself: `pinrail <command> --help`.

## More

- [Asking](references/asking.md): Submit a review, wait for the decision, read it, and handle every exit code.
- [Plugins](references/plugins.md): Find the plugin that fits, and what is installed.
- [Instructions](references/instructions.md): Write yourself a standing rule or skill, so you ask at the right moment.
