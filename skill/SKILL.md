---
name: pinrail
description: {description}
metadata:
  managed-by: Pinrail
  sha: {sha}
  base: {base}
---

# Pinrail

<!-- sessions -->
Pinrail is an app on this computer where a person answers an agent's
questions and reviews what it proposes. You ask with `pinrail ask` and
submit a review with `pinrail submit`. Both wait until the person answers
in the app, then print the answer. Whenever you need the person to answer
something, ask through Pinrail, not in chat, and do not go ahead without
the answer.
<!-- end sessions -->
<!-- no sessions -->
Pinrail is an app on this computer where a person reviews what an agent
proposes. You submit a review with `pinrail submit`, which waits until the
person decides in the app, then prints the decision. Whenever you need the
person to decide something, ask through Pinrail, not in chat, and do not
go ahead without the decision.
<!-- end no sessions -->

If your shell does not find `pinrail`, stop and ask the person to install
the Pinrail command line from the Pinrail app, in Settings › Data. Do not
look for it elsewhere or install it yourself.

<!-- sessions -->
## Which command to use

- `pinrail ask`: you need a short answer before you can go on, such as a
  choice, a fact or a preference. The question fits on one line.
- `pinrail submit`: you made something the person should look at and
  decide on, such as a change, a plan, a design or a list of findings.
- `pinrail status --note`: you want the person to know something, and
  need no answer.

If the answer needs the person to look at your work first, submit it
rather than describe it in a question.

## Asking questions

When you are blocked on something only the person can answer, ask with
`pinrail ask`:

```sh
pinrail ask "Deploy to staging or production?" \
  --choice staging --choice production --recommend staging \
  --ask "Keep the old endpoint for a release?" --yes-no \
  --ask "What should the new endpoint be called?"
```

- Each question takes the options after it, up to the next `--ask`:
  `--choice` once per answer to pick from, `--multiple` to allow several,
  `--yes-no`, `--recommend` for the answer you would choose, and
  `--context` for Markdown that explains it. Without choices, the person
  answers in words.
- A question is one line, and can use inline Markdown: put commands,
  paths and endpoints in backticks.
- Put code, logs and diffs in `--context` inside a fenced block that names
  its language, such as ```` ```diff ````, so that they show as code.
- `--suggest-reply` gives the reply you expect, such as "Yes, commit it
  and go on to PIN-55". The person sends it with Tab, or writes their own.
- Ask up to 10 independent questions at once. A question that depends on
  another's answer goes in a later `pinrail ask`. For questions that
  depend on each other or need long explanations, use the `feedback`
  plugin.
- The command waits, then prints each answer under its question. The
  person may answer in their own words, add notes, comment on them all,
  or leave a question unanswered, which is not a decision.
- The exit codes are those below; `5` means the person declined.

## Reporting progress

Say what you are doing with `pinrail status`. The person sees it in your
session's thread in the app:

```sh
pinrail status --start "Add rate limiting to the public API"
pinrail status "Running the end-to-end suite"
pinrail status --done "Opened pull request #214"
```

- Start each stage of your work with `--start`, naming it as the person would: it is
  your session's title in the app. Report each step that takes more than
  a minute or so. Do not report every command.
- Use `--done` once the person has approved the work and you have
  committed or delivered it, or when the stage ends with nothing to
  review, such as a search. Waiting for a review is a step, not the end.
- The text is one line of at most 200 characters.
- The command does not wait. If it fails, carry on with your work.

The output shows your current stage: if your work has moved on, use
`--start`.

Send a note when the person would want to read it later in the session: a
summary at the end of a stage, a decision you took without asking, or what
an investigation found. A note is Markdown, and asks nothing:

```sh
pinrail status --note --file summary.md --attach latency.png --attach run.log
pinrail status --note "Kept the old endpoint: the nightly job still calls it."
```

- Keep a note to a few lines, and do not repeat what a review you submit
  already shows. Something about a work item is a comment on the item
  instead: see [Work items](#work-items).
- `--attach` sends a file with the note, such as a chart, a screenshot or
  a log. Show an image in place with `![p95 latency](attachment:latency.png)`.
  Every file is also listed under the note.
- A note is at most 10,000 characters, with at most 10 files.

<!-- end sessions -->
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
- `--json`: the whole review as JSON, with the person's answer in
  `decision.data`. Use it only to process the result by program.
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

## Several reviews

If the plugin takes several items in one review, send them together. Otherwise submit each without `--wait`, then run
`pinrail wait <id>` on each. Every wait exits with its own code.

`pinrail show <id>` prints where a review stands. `pinrail withdraw <id>
--reason "<why>"` withdraws one you no longer need. `pinrail list` prints
what is waiting on the person in this project.

## Work items

A work item is a task recorded in Pinrail, such as a feature, a bug or a
step of a plan. It belongs to a repository, which gives it a key such as
`PIN-42`. Inside the repository's checkout, the number alone, `42`, names
it too. An item can have sub-items, and can depend on other items, which
must be done before it is ready.

```sh
pinrail items next --claim          # take the next ready item
pinrail items show PIN-42           # read it before you work on it
pinrail items comment PIN-42 "Found the cause: the cache is per process."
pinrail items done PIN-42 --comment "Fixed in core/src/cache.rs, commit 3f2a1c."
```

- **Read before you work.** Run `pinrail items show <key>` before you
  start or continue an item. Read its description, its comments, the
  decisions of its reviews, and the closing comments of the items it
  depends on.
- **Record a plan of several steps as a breakdown.** Write the steps to a
  JSON file and add them in one request with `pinrail items add --file`.
  Give each step a `ref`, and list in `depends_on` the steps it needs
  first. `pinrail items add --help` shows the file's format.
- **Take the next item** with `pinrail items next --claim` when the
  person asks you to work through the items. Claim a given item with
  `pinrail items edit <key> --claim`. While you hold items, the reviews
  and questions you send belong to all of them. When one is about only
  some of them, name each with `--item`, such as `--item PIN-35 --item
  PIN-36`. The line `submit` and `ask` print on stderr names the items a
  review went under: if they are wrong, send the next round with the
  right `--item`.
- **Follow the person's queue.** `next` offers the items the person
  queued first, in their order; `pinrail items list --queued` shows the
  queue. Only the person changes it.
- **Leave an item on hold alone.** The person puts an item on hold when it
  should wait. `next` never offers it or its sub-items, and claiming one
  is refused. Do not work on it; if you think it should go ahead, ask the
  person to take it off hold.
- **Keep the item current.** Rewrite its description with `pinrail items
  edit <key> --body-file <file>` as the work becomes clearer. Comment on
  your progress, what you found and what you decided, with `pinrail items
  comment`. A comment also shows in your session's thread, so write what
  is about an item as a comment on it, not as a note.
<!-- sessions -->
- **Answer the person's comments.** A comment the person adds to an item
  you hold reaches you in the output of `pinrail status`, like a message.
<!-- end sessions -->
- **Mark it done** with `pinrail items done <key> --comment "…"` once its
  work is approved and committed or delivered. The comment says what
  changed and where, for whoever continues from it. Pinrail refuses while
  a review or question of the item waits, or while a sub-item is open,
  and says what is still open. If you stop before the end, give the item
  back with `pinrail items edit <key> --release`.
- **Leave the order to the person.** Do not reorder or reprioritise work
  you were not asked to. When one item must wait for another, add the
  dependency with `--depends-on`.

## Choosing a plugin

```sh
pinrail plugins                                    # every plugin, a line each: when to use it
pinrail plugins describe <plugin>                  # its payload schema and an example
pinrail plugins describe <plugin> --decision-schema   # what it returns
pinrail plugins skill <plugin>                     # how to build its payload, if it has a skill
```

When a plugin has a skill, read it before you submit to it: it says how to
derive the payload from the work.{plugin_skills}

A plugin fits only if it both shows what the person needs to review and
returns the decision you need: a diff view does not make a plugin fit for
approving a commit if it does not return an approval. Do not bend the task
to fit a schema, or make up an item just to have something to decide on.
Most people have `list`, for items to accept or reject one by one, and
<!-- sessions -->
`feedback`, for questions that depend on each other or need more context
than `pinrail ask` gives.
<!-- end sessions -->
<!-- no sessions -->
`feedback`, for questions to the person.
<!-- end no sessions -->

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

<!-- sessions -->
- Never decide or discard a review you submitted, or answer the questions
  you asked. The person does.
<!-- end sessions -->
<!-- no sessions -->
- Never decide or discard a review you submitted. The person does.
<!-- end no sessions -->
- Every command explains itself: `pinrail <command> --help`.

## More

- [Building a plugin](references/building.md): Make a plugin: create it, design its decision, work on its view with the person, test it, and try it in the app.
