---
title: Code review
description: "The diff of a change and the comments an agent proposes to post on it, reviewed line by line before anything reaches the pull request."
sidebar:
  badge:
    text: Optional
    variant: default
---

<div class="pr-badges"><span class="pr-badge optional">Optional</span><span class="pr-badge plain">plugin: review</span></div>

The code review plugin puts a reviewing agent's comments in front of you before they reach a pull or merge request. You see the diff, with each proposed comment on the line it is about, and decide which ones are worth posting. Your notes improve the ones you keep and explain the ones you reject, so the agent's next review is better than its last.

![The code review plugin: a TypeScript diff with one finding accepted and a note to the agent being written, another rejected with its reason.](screenshot:review "A finding accepted with a revision note, another rejected with its reason.")

## When to use it

- **An AI reviewer on your pull requests**, whose comments you want to filter before your team sees them.
- **Replies to review threads**, where the agent drafts answers to your colleagues' comments.
- **Suggested changes** the agent wants to commit, reviewed as a diff first.

It works with any forge. The agent maps its pull or merge request into the payload, and the decision back out; the plugin itself knows nothing about GitHub or GitLab.

## Install

```sh
pinrail plugins install github.com/forgeplane/pinrail/plugins/review
```

## What you see

- **A file tree** with a count of proposals per file, in the order the agent suggests reading them or by path.
- **The diff**, inline or side by side, with folding and a one-line summary per file.
- **Proposals on their lines**, each with severity, markdown body, and `suggestion` blocks shown as the change they would make. Accept, reject, or add a note.
- **Reply threads**, with the whole conversation so far.
- **Your own comments**, from any line of the diff.
- **A summary before hand-over** showing exactly what goes back.

The layout choices are saved as [plugin settings](/docs/building/settings-and-keys/), so they hold for your next review. Press <kbd>?</kbd> for the keys.

## Asking from your agent

```md title="AGENTS.md"
## Before posting review comments

Never post review comments directly. Submit them to Pinrail as a `review` and
wait for my decision:

1. Write the payload: the change, each file's unified diff (as `git diff`
   prints it), and one proposal per comment, anchored on `file` and `line`.
   Give each proposal a stable integer `id`.
2. Run: `pinrail submit review --title "<PR title>" --origin repo=<owner/repo>,ref=<PR number>,url=<PR URL> --data review.json --wait --format markdown`
3. Post only accepted proposals. Apply an accept note as a revision before
   posting. Never post undecided proposals.
4. Consider my line `comments` and address them in your next round.
5. Learn from rejection notes: don't make the same kind of comment again.
6. If the command exits 5, stop and post nothing.
```

## What the agent sends

```json title="review.json"
{
  "change": {
    "ref": "!42", "title": "Dedup tickets on save", "url": "https://gitlab.example/acme/api/-/merge_requests/42",
    "description": "markdown", "source": "fix/tickets", "target": "main"
  },
  "overview": { "summary": "What changed and why.", "concerns": "What this review focused on." },
  "files": [
    { "path": "lib/acme/tickets.ex", "status": "modified", "summary": "Dedups before insert",
      "rank": 1, "diff": "@@ -140,7 +140,9 @@\n …" }
  ],
  "proposals": [
    { "id": 18, "kind": "comment", "severity": "major", "title": "Reversing twice is a no-op with a cost",
      "body": "The second `Enum.reverse/1` undoes the first.\n\n```suggestion\n|> Enum.uniq_by(& &1.id)\n```",
      "file": "lib/acme/tickets.ex", "line": 149, "side": "new" }
  ]
}
```

- `diff` is one unified diff per file. `rank` sets the reading order.
- `line` counts on `side`, `new` by default. A proposal whose line is not in the diff shows under its file; one whose file is not in the diff shows in a section of its own.
- ` ```suggestion ` blocks in a body render as a proposed change.
- A `kind: "reply"` proposal carries its `thread`, so the conversation renders without the forge.

## What comes back

```json
{
  "decisions": [
    { "id": 18, "action": "accept" },
    { "id": 19, "action": "reject", "note": "out of scope for this change" }
  ],
  "comments": [ { "file": "lib/acme/tickets.ex", "line": 152, "side": "new", "body": "Add a test for this." } ],
  "undecided": [20]
}
```

| Field | What the agent does with it |
|---|---|
| `decisions` | Posts accepted proposals, revised by their `note`. Drops rejected ones, and learns from the reason. |
| `comments` | Your own line comments, for the agent to assess and address. |
| `undecided` | Never posted. |

## Reference

The plugin's manifest, and the schemas a payload and a decision are checked against, read from the plugin's own files.

![The Code review plugin's contract](contract:review)
