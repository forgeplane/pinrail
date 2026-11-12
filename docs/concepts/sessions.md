---
title: Sessions
description: "What an agent session is, what it sends you, and how its questions, statuses and notes differ from reviews."
---

A session is one run of an agent, such as a Claude Code conversation or a Codex session. Everything the agent sends you from that run belongs to it, and Pinrail shows it as one thread, in the order it happened. A session lets you see what an agent is doing and answer it without leaving the context of its work.

:::tip[What a thread records]
A session's thread is not a transcript of the agent's conversation. Pinrail records only the moments that carry information for you: the questions the agent asks, the work it submits for review, the stage it starts and finishes, a short status as it goes, and the notes it chooses to send. Everything else the agent writes stays in its terminal. With several agents at work, the threads give you an overview of what each is doing and what waits for you, without reading every line they produce.
:::

## How Pinrail knows the session

The `pinrail` command reads the agent and its session from the agent's environment, such as `CLAUDE_CODE_SESSION_ID` for Claude Code, and sends them with each review, question and status. The agent does nothing to name its session. A review or a question sent from no agent, such as from a CI job, belongs to no session and shows only in the inbox.

## What a session sends you

| What | Sent with | Waits for you | What the agent gets back |
|---|---|---|---|
| **Questions** | `pinrail ask` | Yes, in the inbox | Your answers, your notes and your comment |
| **Reviews** | `pinrail submit` | Yes, in the inbox | Your decision, as the plugin defines it |
| **Statuses** | `pinrail status` | No | Nothing |
| **Notes** | `pinrail status --note` | No | Nothing |

Questions and reviews ask for your answer: the agent waits until you give it. Statuses and notes only tell you something.

## Questions

A question is a short request for an answer the agent needs before it can go on, such as a choice, a fact or a preference. Under the hood, a set of questions is a [review](/docs/concepts/reviews/) of its own kind: it waits in the inbox, ends in the same ways, and the command that asked it exits with the same codes. It needs no plugin, and you answer it in the session's thread rather than in a plugin's view.

The questions an agent asks together are one set, and you send their answers together. You can answer in your own words, add a note to any answer, and add a comment on the whole set. A question you leave alone goes back as not answered, which is not a decision. Declining the set tells the agent to stop, as discarding a review does.

## Statuses and stages

A status is one line that says what the agent is doing, such as *Running the integration tests*. It shows in the thread at the moment it was sent, and the newest status shows on the Sessions page and in the sidebar.

An agent groups its work into **stages**. It starts a stage with `pinrail status --start`, which names it, and the stage becomes the session's title. It ends the stage with `pinrail status --done` once the work is approved and delivered, or when nothing needs a review. A session can have several stages, one after the other.

## Notes

A note is a message in Markdown that tells you something and asks nothing, such as a summary of a stage, a decision the agent took on its own, or what an investigation found. It can carry files, such as a chart or a log, and shows images in place.

## Sessions and work items

A session can hold [work items](/docs/concepts/work-items/). While it holds them, the reviews and questions it sends belong to them, and its thread shows the changes it makes to items and its comments on them. A comment you add to an item the session holds reaches the agent the next time it runs `pinrail status`.

## The life of a session

A session appears with the first thing its agent sends, and stays on the Sessions page and in the sidebar until you archive it. Archiving deletes nothing: the session leaves the lists, and its questions and reviews stay in the inbox while they wait. A session whose agent sends something again comes back by itself.

When you set *Keep reviews for* in *Settings › Data*, Pinrail deletes old reviews from the history, and with them each session that has no review left and no activity in that time. Deleting a session releases the work items it held.

:::note[Current limitations]
You cannot drive an agent from Pinrail. You start a session, give the agent its instructions and talk to it in the agent's own terminal or app, as before. Pinrail shows what the session sends and takes your answers to its questions and reviews, but it has no box for a new prompt to the agent. A comment on a work item the session holds reaches the agent only the next time it runs `pinrail status`.
:::

## Where to go next

- [Questions and sessions](/docs/using/sessions/): answering questions, the thread, and the keys to use.
- [Agents › CLI](/docs/agents/cli/): `pinrail ask` and `pinrail status`, with every option.
- [Reviews](/docs/concepts/reviews/): what a review is, and how it ends.
