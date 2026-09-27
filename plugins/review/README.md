# review

The code review plugin shows a change's diff with the comments a reviewing
agent proposes to post, each attached to a line. The person accepts or
rejects each proposal, optionally with a note, which is an instruction to
revise on an accepted proposal and the reason on a rejected one. The person
can also add line comments of their own before handing the review over.

The plugin works with any code host. The agent converts its pull request or
merge request into the payload, and converts the decision back into
comments on the host.

## Payload

```json
{
  "change": { "ref": "!42", "title": "Dedup tickets on save", "url": "https://…",
              "description": "markdown", "source": "fix/tickets", "target": "main" },
  "overview": { "summary": "what and why, markdown", "concerns": "what the review focused on" },
  "files": [
    { "path": "lib/acme/tickets.ex", "status": "modified", "summary": "one line",
      "rank": 1, "diff": "@@ -140,7 +140,9 @@\n …unified diff…" }
  ],
  "proposals": [
    { "id": 18, "kind": "comment", "severity": "major", "title": "…", "body": "markdown",
      "file": "lib/acme/tickets.ex", "line": 149, "side": "new" },
    { "id": 19, "kind": "reply", "severity": "minor", "title": "…", "body": "…",
      "file": "lib/acme/tickets.ex", "line": 31, "resolves": true,
      "thread": { "round": 1, "comments": [ { "author": "us", "body": "…", "ours": true },
                                             { "author": "alice", "body": "…" } ] } }
  ]
}
```

- `diff` is one unified-diff string per file, as `git diff` prints it.
  `rank` gives the semantic reading order; without ranks files sort by path.
- `line` counts on `side` (`new` by default). A proposal whose line is not
  in the diff renders under its file as a file-level comment; one whose file
  is not in the diff renders in a section of its own.
- ```` ```suggestion ```` blocks in a body render as a proposed change,
  with the `:-A+B` range modifier honoured when present.
- `thread` carries the whole conversation for a reply, so history renders
  without the forge. `ours` marks the requester's own comments; threads with
  a developer's response open by default.

## Decision

```json
{
  "decisions": [ { "id": 18, "action": "accept" },
                 { "id": 19, "action": "reject", "note": "out of scope" } ],
  "comments": [ { "file": "lib/acme/tickets.ex", "line": 152, "side": "new", "body": "markdown" } ],
  "undecided": [ 20 ]
}
```

- On accept, `note` is a revision instruction the agent applies before
  posting; on reject, the reason, which becomes a lesson.
- `comments` are the reviewer's own line comments, for the agent to assess.
- `undecided` lists every proposal without a verdict; the requester must not
  post them. Submitting with undecided proposals asks for confirmation.

## View

File tree with per-file counts, semantic or path order, inline or split
diff, folding, per-file summaries, proposals anchored on their lines with
accept / reject / note, reply threads, suggestion blocks, the reviewer's
own comments from any diff line, keyboard navigation
(`?` lists the keys), a submit summary that shows exactly what goes back,
drafts across reloads, read-only rendering with verdicts overlaid, and the
previous round's verdict on each proposal when the review supersedes another.

The view asks the shell for a viewport-height frame and scrolls inside it.

## Settings

The manifest declares five, shown under *Code review* in *Settings ›
Plugins*: the diff inline or side by side, whether long lines wrap or the
file scrolls sideways (inline only; side by side always wraps), files in
the agent's order or by path, only files with findings, and whether the
tree starts open. The controls in the view and the V, W and O keys change
the same settings, so a choice made while reviewing holds for the next
review too.
