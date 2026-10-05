# Markdown review

A Markdown document under review, shown with its outline: rendered with
its Mermaid diagrams, or as its raw source with line numbers. The person
comments on a section (from its heading), on a diagram, or on a passage
they select in either view. Each comment is a change to make or a
question for the agent to answer. Handing over with no comments approves
the document, with any change requests changes, and with questions alone
asks the agent to explain without changing it. Each comment goes back as
lines of the payload's markdown, with the headings it sits under and, for
a passage, the text selected.

```sh
pinrail plugins install ./markdown --link
pinrail submit markdown --sample
pinrail submit markdown --title "Design: retries" --data payload.json --wait
```

The payload is `{"markdown": "…", "path": "docs/retries.md", "context": "…"}`.
Instead of `markdown`, the document can come as a file sent with `--attach`,
named in `file`: `{"file": {"$attachment": "retries.md"}, "path": …}`. One of
the two is required. Raw HTML in the document is shown as text.

```sh
pinrail submit markdown --title "Retry policy" --data review.json \
  --attach docs/retries.md --wait
```

## Building

The view's source is in `src/`, and Vite builds it into `view/`, which Git
does not track. markdown-it and Mermaid come from npm and are bundled with
it, since a view loads nothing from outside its folder. Mermaid is a chunk
of its own, fetched only for a document with a diagram.

```sh
npm ci
npm run build
```

From the repository root, `mise run plugins:build` builds every plugin that
has a build of its own. The app's build needs it first.
