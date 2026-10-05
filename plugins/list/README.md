# list

The list plugin shows items grouped under headings. The person accepts or
rejects each item, optionally with a note, and any item left without a
verdict is reported in `undecided`. Anything else the person wants to say
goes in the review's note to the agent.

The plugin is built into the app, so every agent can use it without
installing anything. It is also the most complete example of the plugin
protocol, with drafts, read-only rendering and the previous round's
verdicts.

## Payload

```json
{
  "summary": "markdown shown above the list",
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

`id` is the workflow's own integer and is never renumbered. `summary`,
optional, is Markdown shown above the list in a box the person can fold
away. `severity` is free text: `blocker`, `major`, `minor` and `nit` have
colours of their own, and `severities` gives others, or changes those, by
naming the tone each one is shown in:

```json
"severities": { "critical": "danger", "high": "warning", "low": "info", "fixed": "success" }
```

The tones are `danger`, `warning`, `info`, `success` and `neutral`. A
severity that neither names is neutral. `body` is Markdown. `meta` is shown
as `key: value` chips, and its values are strings, numbers or booleans.

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

`undecided` is required and lists every item the person left without a
verdict. Submitting with undecided items asks for confirmation first. A
requester must treat those as not approved. On accept, `note` is a revision
instruction; on reject, the reason.

## Behaviour

- Clicks post a draft at once and typing is debounced, so a reload restores
  the work in progress.
- ⌘/Ctrl+Enter in the shell submits.
- Read-only after the decision, or when the review is withdrawn or expired.
- On a review that supersedes another, each item shows the previous round's
  verdict for the same id.
