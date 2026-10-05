---
title: List
description: "Items grouped under headings, each accepted or rejected with a note. The general-purpose plugin for findings, tasks and proposed actions."
sidebar:
  badge:
    text: Built in
    variant: success
---

<div class="pr-badges"><span class="pr-badge built-in">Built in</span><span class="pr-badge plain">plugin: list</span></div>

The list plugin shows a set of items, grouped under headings, and asks for a verdict on each: accept or reject, with an optional note. It is the general-purpose plugin, and the one to reach for first. It ships with the app, so every agent can use it with nothing to install.

![The list plugin: dependency upgrades grouped as safe, needs a look and hold, with three accepted, one held back with a reason, and a note being written on a major upgrade.](screenshot:list "Dependency upgrades: the safe ones accepted, one held back, a note on the major upgrade.")

## When to use it

- **Triage.** Errors, alerts or tickets the agent proposes to mute, fix, close or escalate.
- **A plan.** The steps an agent is about to take, so you can strike the ones you don't want before it starts.
- **Findings.** Security or dependency audit results, lint findings, flaky tests to quarantine.
- **Batch changes.** Records to update, files to delete, branches to prune, invitations to send.

If the items need a richer view, such as a diff for each, look at [Code review](/docs/plugins/review/) or [build a plugin](/docs/building/writing/).

## What you see

- **A summary** from the agent, when it gives one, in a box you can fold away.
- **Groups** with their headings, and each item's title, description and details.
- **Severity**, when the agent gives one. `blocker`, `major`, `minor` and `nit` have colours of their own, and the agent can give other severities a colour too; anything else shows in grey.
- **A sidebar** with each group's items. An item's circle fills once you accept it, and a rejected item is struck through. Click an item to go to it.
- **Accept** or **Reject** on each item, with a note. On an accepted item the note is a revision instruction; on a rejected one, the reason.
- **Earlier verdicts.** When the review is a new round, each item shows the verdict you gave it last time.

Anything you leave undecided is reported as undecided. When you hand over with items left, the list asks you to confirm first, and the hand-over button says how many go back undecided.

To see it before any agent asks with it, send its sample: `pinrail submit list --sample`, or **Send a sample** in its details in *Settings › Plugins*.

## Asking from your agent

Paste this into your agent's instructions and adjust the first line to the moment you want it to ask:

```md title="AGENTS.md"
## Before acting on a batch of changes

Before you close, mute or change more than one item, ask me with Pinrail's
`list` plugin and wait for my decision:

1. Write the items to a JSON file: `{ "summary": "…", "groups": [{ "title": "…",
   "items": [{ "id": 1, "severity": "major", "title": "…", "body": "…" }] }] }`.
   Give each item a stable integer `id` and say in `body` what you will do.
2. Run: `pinrail submit list --title "<what this is>" --data items.json --wait`
3. Act only on items marked accepted, applying any note as an instruction.
   Rejected and undecided items are not approved: leave them.
4. If the command exits 5, the review was discarded: stop and tell me why.
```

See [Instructing an agent](/docs/agents/instructing/) for where these instructions go for each agent.

## What the agent sends

```json title="items.json"
{
  "summary": "Sentry triage for **acme-api**, last 7 days.",
  "groups": [
    {
      "title": "Mute",
      "items": [
        { "id": 101, "severity": "minor", "title": "Mute NullPointer in /checkout for 7 days",
          "body": "Fires 40 times an hour since the 3.2 deploy; the fix is in review.",
          "meta": { "events": "1.2k" } }
      ]
    },
    {
      "title": "Open an issue",
      "items": [
        { "id": 103, "severity": "major", "title": "Timeout on /export past 50k rows",
          "body": "Nine users hit it this week. The query has no index on `exported_at`." }
      ]
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `summary` | Markdown shown above the list, in a box the person can fold away. Optional. |
| `groups[].title` | The heading items are grouped under. |
| `items[].id` | The agent's own integer. Never renumbered, so verdicts from one round match the next. |
| `severities` | The colour each severity this payload uses is shown in, such as `{ "critical": "danger", "low": "info" }`. The colours are `danger`, `warning`, `info`, `success` and `neutral`. Optional. |
| `items[].severity` | Free text. `blocker`, `major`, `minor` and `nit` have colours of their own, which `severities` can change. Any other severity is grey unless `severities` names it. |
| `items[].title`, `items[].body` | What the item is. `body` is markdown. |
| `items[].meta` | Extra details, shown as `key: value` chips. Each value is a string, a number or a boolean. |

## What comes back

```json
{
  "decisions": [
    { "id": 101, "action": "accept", "note": "mute it for 3 days, not 7" },
    { "id": 103, "action": "reject", "note": "known, already scheduled" }
  ],
  "undecided": []
}
```

- `decisions` has one entry per item with a verdict. `note` is a revision instruction on `accept` and the reason on `reject`.
- `undecided` lists every item left without a verdict. Treat those as not approved.

In Markdown, which is the default, the agent reads the same decision under each group's heading: every item with its verdict and title, the person's note below it, and the items without a verdict marked as not approved.

## Reference

The plugin's manifest, and the schemas a payload and a decision are checked against, read from the plugin's own files.

![The List plugin's contract](contract:list)
