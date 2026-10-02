---
title: Reviews
description: "What a review is, how it ends, and how rounds let an agent answer your feedback."
---

A review is a single request from an agent for your decision on work that it is about to do. It carries the work to decide on, waits in your inbox, and ends with an outcome that the agent can act on.

## What a review holds

| Part | What it is |
|---|---|
| **Title** | What the review is about, as it appears in your inbox. |
| **Plugin** | The kind of review: [List](/docs/plugins/list/), [Code review](/docs/plugins/review/), and so on. It decides the view and the shape of the decision. |
| **Payload** | The work to decide on: the items, the diff, the drafts. Checked against the plugin's schema when the review is created. |
| **Attachments** | Files sent with the payload, for a plugin that accepts them, such as a 3D model, a PDF or photos. They are stored with the review and deleted with it. The app lists them as *files*. |
| **Origin** | Where it comes from: a repository, a workflow, a run, a branch or pull request, a link. The inbox groups and filters by it. |
| **Requester** | Who is asking, such as an agent's name or a CI job. |
| **Decision** | Your answer, once you give it, and your note to the agent beside it. |

## How a review ends

Every review starts **pending** and ends exactly one way.

```mermaid title="The life of a review"
stateDiagram-v2
  direction LR
  [*] --> pending: the agent submits
  pending --> decided: you hand over a decision
  pending --> discarded: you say no, and stop
  pending --> withdrawn: the agent takes it back
  pending --> expired: its deadline passes
  decided --> [*]
  discarded --> [*]
  withdrawn --> [*]
  expired --> [*]
```

| Outcome | Who | What the agent gets |
|---|---|---|
| **Decided** | You | Your decision and your note. The waiting command exits `0`. |
| **Discarded** | You | Your reason, and an instruction to stop the work. Exit `5`. |
| **Withdrawn** | The agent | Nothing to act on. The agent withdrew the review before you decided it. Exit `3`. |
| **Expired** | Nobody | The review had a deadline and nobody decided in time. Exit `3`. |

An ended review never changes again. It moves from the inbox to your history, read-only, and renders with the release of the plugin it was submitted to.

### Deciding and discarding

**Deciding** is a considered answer: accept this, reject that, change these words. Whatever the plugin lets you say, you say it, and the agent acts on it.

**Discarding** is different: it means *no, and stop*. Use it when the whole thing is wrong, not worth doing, or not wanted now. You can give a reason, which goes to the agent. A discarded review has no decision, and an agent that is told it was discarded should stop the work it was asking about rather than try again.

## Rounds

Often you want some changes before you approve. The agent makes the changes and asks again, submitting a new review that **revises** the first. That is a new round.

```mermaid title="Two rounds of one review"
flowchart LR
  R1["round 1<br/>pending"] -->|"you decide: change #19"| D1["round 1<br/>decided"]
  D1 -.->|"the agent revises"| R2["round 2<br/>pending"]
  R2 -->|"you accept"| D2["round 2<br/>decided"]
```

![Round two of a code review: a finding from round one, with its previous verdict and note, above the agent's answer to it.](screenshot:rounds "Round 2: the earlier verdict and note beside the agent's answer.")

- In the app, a new round shows your previous verdicts beside each item, so you can see what changed and only look again where you need to.
- <kbd>[</kbd> and <kbd>]</kbd> step between rounds of the same review.
- Every round is kept. Each is its own review with its own outcome, linked to the one it revises.

The rounds of a review form a single line. A new round revises the latest round of its review and uses the same plugin, and Pinrail refuses a round that does not. If the round it revises is still waiting for a decision, Pinrail withdraws that round, with the reason *superseded by* the new round's id, so the agent still waiting on it is told and your inbox shows only the new round.

An agent submits a new round with `--revises <id>`. See [Instructing an agent](/docs/agents/instructing/#rounds).

## Your note to the agent

Besides the plugin's decision, every hand-over can carry a note to the agent: context that belongs to the review as a whole rather than to any one item. The agent receives it with the decision, and it is shown in your history.
