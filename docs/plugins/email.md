---
title: Email
description: "Drafts an agent wants to send, edited with the changes showing, then sent, revised or discarded one by one."
sidebar:
  badge:
    text: Optional
    variant: default
---

<div class="wk-badges"><span class="wk-badge optional">Optional</span><span class="wk-badge plain">plugin: email</span></div>

The email plugin stops an agent from sending anything you haven't read. Each draft opens like a message you are about to send: you edit it in place, with your changes showing against what the agent wrote, comment on the passages that need work, and decide whether it goes out.

![The email plugin: a draft whose opening line was rewritten, with the change showing against the original, and a passage commented.](screenshot:email "An edit that shows against the draft, and an instruction on one passage.")

## When to use it

- **Customer email** an agent drafts from your CRM or support queue.
- **Outreach and follow-ups**, where tone matters as much as content.
- **Replies in a thread**, with the conversation so far shown under the draft.

It works with any mail provider. The agent maps its mailbox into the payload and the decision back out.

## Install

```sh
wicket plugins install github.com/pnezis/wicket/plugins/email
```

## What you see

- **Every draft down a rail**, like an inbox: who it is to, the subject, the first line, and its verdict so far.
- **The open draft as it would be sent**: from, to, cc and bcc, the subject and the body, with the agent's reason for writing it this way above it and any thread it replies to.
- **Edits in place, tracked.** The message is editable where it stands, like suggestions in a document editor: what you cut stays on screen struck through and what you add is marked, so both versions are visible at once. Every change, to the subject or the message, is listed in a panel on the right, where each can be put back on its own and *Revert all* restores the agent's words. Undo and redo walk through your edits in the order you made them.
- **Instructions on passages.** Select any text and a comment button appears beside it; write the instruction in the popover over the passage. The passage is numbered in the body, and the instruction with the same number is listed in the panel on the right, under the changes.
- **A verdict per draft**, kept at the foot of the page: *Send* this text as it stands, *Revise* it from your instructions, or *Discard* it, with an optional note on the draft as a whole.

Keys: `j` / `k` next and previous draft, `s` send, `r` revise, `x` discard, `shift+s` send every draft still undecided, `e` put the cursor in the message.

## Asking from your agent

```md title="AGENTS.md"
## Before sending email

Never send email directly. Submit drafts to Wicket as `email` and wait:

1. Write the payload: `from`, and one draft per message with `id`, `to`,
   `subject`, `body` (plain text, as it would be sent) and `why`, your reason
   for writing it this way.
2. Run: `wicket submit email --title "<what these emails are>" --data drafts.json --wait --format markdown`
3. For `send`, send the returned `subject` and `body` exactly as they are.
   For `revise`, rewrite from my comments and note, and submit a new round
   with `--revises <id>`. For `discard`, drop it. Never send an undecided draft.
4. Read my `edits`: they show how I write. Write that way next time.
5. If the command exits 5, send nothing.
```

## What the agent sends

```json title="drafts.json"
{
  "from": "sam@acme.com",
  "intro": "Renewals due this month. I kept these short and did not offer discounts.",
  "drafts": [
    {
      "id": "northwind",
      "to": ["priya@northwind.example"], "cc": [], "bcc": [],
      "subject": "Your Acme renewal on 12 October",
      "body": "Hi Priya,\n\nI wanted to reach out ahead of your renewal on 12 October…",
      "why": "Usage is up 40% and they have never raised a ticket, so I kept it short.",
      "thread": [{ "author": "priya@northwind.example", "at": "12 June", "body": "…" }]
    }
  ]
}
```

## What comes back

```json
{
  "drafts": [
    {
      "id": "northwind", "action": "send",
      "subject": "Your Acme renewal on 12 October",
      "body": "…the final text…",
      "edits": [{ "from": "at your earliest convenience", "to": "this week" }],
      "comments": [{ "quote": "I wanted to reach out", "note": "we don't say reach out" }],
      "note": "Good otherwise."
    }
  ],
  "undecided": ["kestrel"]
}
```

| Field | Meaning |
|---|---|
| `action` | `send`, `revise` or `discard`. |
| `subject`, `body` | The final text, including your edits. On `send`, this is what goes out. |
| `edits` | What you changed, before and after. The part worth learning from. |
| `comments` | Instructions pinned to quoted passages. |
| `note` | A note on the draft as a whole. |
| `undecided` | Drafts without a verdict. Nothing is sent for them. |
