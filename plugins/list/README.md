# list

The built-in gate type: items grouped under headings, accept or reject each
with an optional note, and an honest `undecided` list. Anything else the
person wants to say goes in the review's own note to the agent.
Ships inside the app; every workflow can use it before it has a view of its
own. It is also the fullest reference client of the plugin protocol: drafts,
read-only rendering, the previous round's verdicts.

## Payload

```json
{
  "intro": "markdown shown above the list",
  "groups": [
    {
      "title": "lib/acme/tickets.ex",
      "items": [
        { "id": 18, "severity": "major", "title": "do_save dedups without reversing",
          "body": "markdown", "meta": { "line": 149 } }
      ]
    }
  ]
}
```

`id` is the workflow's own integer and is never renumbered. `severity` is
free text; `blocker`, `major`, `minor` and `nit` get colours. `meta` is shown
as key: value chips.

## Decision

```json
{
  "decisions": [
    { "id": 18, "action": "accept" },
    { "id": 19, "action": "accept", "note": "revise: mention the COALESCE" },
    { "id": 20, "action": "reject", "note": "fine for a doc, don't nitpick" }
  ],
  "undecided": [ 17 ]
}
```

`undecided` is required and lists every item the human left without a
verdict. Submitting with undecided items asks for confirmation first. A
requester must treat those as not approved. On accept, `note` is a revision
instruction; on reject, the reason.

## Behaviour

- Clicks post a draft at once and typing is debounced, so a reload restores
  the work in progress.
- ⌘/Ctrl+Enter in the shell submits.
- Read-only after the decision, or when the gate is withdrawn or expired.
- On a gate that supersedes another, each item shows the previous round's
  verdict for the same id.
