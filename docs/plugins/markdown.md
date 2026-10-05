---
title: Markdown review
description: "A Markdown document an agent wrote, such as a plan, a spec or a README, read with its diagrams and commented on section by section before the agent goes on."
sidebar:
  badge:
    text: Optional
    variant: default
---

<div class="pr-badges"><span class="pr-badge optional">Optional</span><span class="pr-badge plain">plugin: markdown</span></div>

The Markdown review plugin shows a document an agent wrote, such as a plan, a design document, a README or release notes, and lets you comment on it before the agent goes on. You read it rendered, with its Mermaid diagrams, or as its source with line numbers. Each comment asks for a change or asks the agent a question, and goes back as lines of the document, so the agent knows exactly what it is about.

![The Markdown review plugin: a design document's Design section with its diagram, a question on the diagram and a change asked for on a section.](screenshot:markdown "A question on the diagram, and a change asked for on the Backoff section.")

## When to use it

- **A plan** the agent wants to follow, read and corrected before it starts.
- **Design documents and specs**, with their diagrams.
- **READMEs, changelogs and release notes**, checked before they are published.

## Install

The plugin comes with the app. Install it in *Settings › Plugins*, or from the command line:

```sh
pinrail plugins install markdown
```

## What you see

- **The document, rendered** with its tables, code and Mermaid diagrams, or **as its raw source** with line numbers.
- **The outline** of its headings beside it, with a count of the comments under each. The button at the start of the header hides it, and the choice is kept as a setting.
- **The agent's context** above the document, when it sends some, such as what changed since the last round.
- **Comments** on a section, from its heading, on a diagram, or on a passage you select in either view. Each comment is a **Change** or a **Question**.
- **The previous round's comments**, when the agent sends a new version.

There is no verdict to choose. Handing over with no comments approves the document. With any change, it asks for changes. With questions alone, it asks the agent to explain without changing the document.

To see it before any agent asks with it, send its sample: `pinrail submit markdown --sample`, or **Send a sample** in its details in *Settings › Plugins*.

## Asking from your agent

```md title="AGENTS.md"
## Before acting on a plan or a document

When you write a plan, a spec or a document I should read, submit it to
Pinrail with the `markdown` plugin and wait for my decision:

1. Run: `pinrail submit markdown --title "<what the document is>" --data review.json --attach <the file> --wait`,
   with `{"file": {"$attachment": "<the file's name>"}, "path": "<its path>"}` in review.json.
2. If the verdict is `approve`, go on.
3. If it is `request_changes`, apply each `change` comment, answer each
   `question`, and submit the next version with `--revises <id>`.
4. If it is `questions`, answer them without changing the document, and
   submit it again with `--revises <id>`.
5. If the command exits 5, stop.
```

## What the agent sends

```json title="review.json"
{
  "file": { "$attachment": "retries.md" },
  "path": "docs/retries.md",
  "context": "First draft. Mostly the **Design** section needs a look."
}
```

- The document is a file sent with `--attach` and named in `file`, or its text in `markdown` in place of `file`. One of the two is required.
- `path` is how the person knows the document, shown in the header.
- `context` is markdown, shown above the document.
- Diagrams are ` ```mermaid ` blocks. Raw HTML in the document is shown as text.

## What comes back

```json
{
  "verdict": "request_changes",
  "comments": [
    { "id": 1, "kind": "change", "body": "Show the delays up to the eighth try.",
      "target": { "kind": "section", "start_line": 25, "end_line": 39,
                  "section": "Retry policy for webhook deliveries › Design › Backoff" } },
    { "id": 2, "kind": "question", "body": "Where does Retry-After come in?",
      "target": { "kind": "diagram", "start_line": 16, "end_line": 23, "quote": "flowchart LR" } }
  ]
}
```

| Field | What the agent does with it |
|---|---|
| `verdict` | `approve`: goes on. `request_changes`: applies the changes and answers the questions in a new version. `questions`: answers the questions and sends the same version again. |
| `comments[].kind` | `change` is something to change in the document. `question` is something to explain. |
| `comments[].target` | What the comment is about: a `section`, a `diagram` or a `text` passage, as lines of the document, with the headings it sits under and, for a passage, the text selected. |

## Reference

The plugin's manifest, and the schemas a payload and a decision are checked against, read from the plugin's own files.

![The Markdown review plugin's contract](contract:markdown)
