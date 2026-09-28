---
title: Feedback
description: "Questions an agent wants answered before it goes on, grouped and conditional, answered in one pass."
sidebar:
  badge:
    text: Built in
    variant: success
---

<div class="pr-badges"><span class="pr-badge built-in">Built in</span><span class="pr-badge plain">plugin: feedback</span></div>

The feedback plugin is how an agent asks you questions. It sends a short form: choices, yes-or-no questions, free text and acknowledgments, in groups, with follow-up questions that appear only when an earlier answer calls for them. You answer everything in one pass and hand it back. It ships with the app, so there is nothing to install.

![The feedback plugin: questions about paginating an orders API, with the recommended answers chosen and a comment being written on the first.](screenshot:feedback "Questions before a change to an API: the agent's recommendations, the answers, and a comment.")

## When to use it

- **Before starting work.** Preferences the agent should not guess: which approach, which audience, how far to go.
- **At a fork.** A choice between options the agent has worked out, with its recommendation and reasoning.
- **Before something irreversible.** An explicit acknowledgment, such as "I understand this deletes the staging data".
- **Requirements gathering.** A structured interview in place of a long back-and-forth in chat.

Use feedback when the agent needs *information* from you. When it needs a *verdict* on things it proposes to do, use [List](/docs/plugins/list/).

## What you see

- **Groups** of numbered questions, each question a card that fills the panel. A rail on the left lists every group and its questions, with a dot for where each stands, and jumps to any of them.
- **The agent's recommendation**, marked on the option it suggests and set out beside the question with its reasoning. Nothing is preselected: the answer is yours, and *Use this answer* takes the recommendation in one click.
- **Follow-up questions** that appear when an earlier answer calls for them, and hide again if you change it.
- **A comment** on any choice question, to qualify your answer.
- **Your previous answers**, for reference, when the agent asks again in a new round.

Keys: <kbd>j</kbd> / <kbd>k</kbd> move to the next and previous question and put the focus on its answer, so the arrow keys or space answer it.

The app asks you to complete required questions before you hand over.

To see it before any agent asks with it, send its sample: `pinrail submit feedback --sample`, or **Send a sample** in its details in *Settings › Plugins*.

## Asking from your agent

```md title="AGENTS.md"
## When you need a decision from me

When you need my input to go on, don't guess and don't ask in chat. Ask with
Pinrail's `feedback` plugin and wait:

1. Write the questions to a JSON file (the shape is below). Give every group
   and question a stable `id`. Offer choices when you can, and add a
   `recommendation` with your reasoning when you have one.
2. Run: `pinrail submit feedback --title "<what you need to decide>" --data questions.json --wait`
3. Use the answers as given. An answer's comment qualifies it; read it.
   Questions under `unanswered` got no answer: don't assume one.
4. If the command exits 5, stop and tell me why.
```

## What the agent sends

```json title="questions.json"
{
  "description": "Checkout is failing for 4% of users since the 3.2 deploy. I need a plan before I touch production.",
  "groups": [
    {
      "id": "recovery",
      "title": "Recovery",
      "questions": [
        {
          "id": "approach",
          "type": "single_choice",
          "prompt": "How should I proceed?",
          "required": true,
          "options": [
            { "id": "rollback", "label": "Roll back to release 2.7", "description": "Fastest recovery." },
            { "id": "patch", "label": "Apply the proposed patch", "description": "Keeps the new checkout flow." }
          ],
          "recommendation": { "answer": "rollback", "reason": "The previous release has a known-good payment path." }
        },
        {
          "id": "preserve_logs",
          "type": "checkbox",
          "prompt": "Preserve logs before rolling back",
          "checkbox_label": "Keep the last 24 hours of payment logs",
          "required": true,
          "when": { "question_id": "approach", "operator": "equals", "value": "rollback" }
        }
      ]
    }
  ]
}
```

### Question types

| `type` | Answer | Options |
|---|---|---|
| `single_choice` | One option `id` | `options: [{ id, label, description? }]` |
| `multiple_choice` | An array of option ids | `options`, `min_selections`, `max_selections` |
| `text` | A string | `placeholder`, `min_length`, `max_length` |
| `boolean` | `true` or `false` | Shown as Yes and No. No counts as an answer. |
| `checkbox` | `true` or `false` | `checkbox_label`. A required one must be checked. |

Every question takes `prompt`, optional `description` (markdown) and `required`. Questions are optional unless `required: true`.

### Follow-up questions

A question or a whole group can have a `when` condition on an earlier question:

```json
{ "all": [
  { "question_id": "notify", "operator": "equals", "value": true },
  { "question_id": "channels", "operator": "contains", "value": "email" }
] }
```

Operators are `equals`, `not_equals`, `contains` (for multiple choice) and `answered`. Combine them with `all` and `any`. A condition can only refer to questions that come before it.

:::note
An unanswered or hidden question never satisfies a condition, not even `not_equals`. Follow-ups stay hidden until the person has actually answered the question they depend on.
:::

## What comes back

```json
{
  "answers": [
    { "question_id": "approach", "answer": "rollback", "comment": "Preserve logs first." },
    { "question_id": "preserve_logs", "answer": true, "comment": "" }
  ],
  "unanswered": [],
  "excluded": []
}
```

- `answers` holds each answered question in order, with the person's comment.
- `unanswered` lists optional questions the person skipped.
- `excluded` lists questions hidden by conditions. Their answers are never included.

There is no overall approve or reject: the answers are the decision, and the agent acts on them.

## Reference

The plugin's manifest, and the schemas a payload and a decision are checked against, read from the plugin's own files.

![The Feedback plugin's contract](contract:feedback)
