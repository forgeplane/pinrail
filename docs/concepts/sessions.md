---
title: Sessions
description: "What an agent session is, where its agent runs, how Pinrail knows what the agent is doing, and how you send the agent messages."
---

A session is one run of an agent, such as a Claude Code conversation or a Codex session. Everything the agent sends you from that run belongs to it, and Pinrail shows it as one thread, in the order it happened. The thread is a summary, not a transcript of the agent's conversation. A session also shows what its agent is doing now, and it is where you send the agent messages. The [Overview](/docs/concepts/overview/) shows how sessions connect to the inbox and to work items.

## Where a session runs

An agent runs in Pinrail or in your own terminal, and its session differs in what you can do with it.

| | A session that Pinrail runs | A session in your own terminal |
|---|---|---|
| **How it starts** | You start the agent from the **New session** page or from a work item's panel. | You start the agent as usual. |
| **The terminal** | The session's **Terminal** view shows the agent's terminal and takes your typing. | Pinrail does not show the terminal. You type to the agent where it runs. |
| **Stopping the agent** | **Stop** in the session's header stops the agent. | You stop the agent in its own terminal. |
| **Your messages** | Pinrail types them into the agent's terminal. | The agent receives them the next time it runs `pinrail status`. |

Pinrail runs an agent in its terminal host, a separate program that keeps the agent running while the app is closed. Pinrail starts and resumes Claude Code, Codex, OpenCode, Copilot CLI, Cursor, Grok CLI, Pi and Antigravity CLI.

Pinrail only watches a session in your own terminal. It shows what the agent reports and sends, and you answer the agent's questions and reviews in the app. Once such a session has stopped, you can resume it in Pinrail, which then runs it.

:::note[Agents keep running when you quit Pinrail]
When you quit Pinrail while agents run in it, Pinrail asks whether to leave them running or to stop them first. The agents that you leave running carry on with their work, and you can open them again when you reopen Pinrail. To make Pinrail stop asking, choose what it does under **Quitting with agents running** in *Settings › General*.
:::

## The Summary and Terminal views

A session that Pinrail runs has three views. The buttons at the right of the session's header switch between them.

| View | What it shows |
|---|---|
| **Summary** | The session's thread, which holds what matters to you: the agent's questions and reviews, its statuses and notes, a permission or an answer that it waits for, and its changes to work items. When the agent reports its status, the thread also shows your prompts and the agent's reply at the end of each turn. The summary leaves out the rest of the agent's output, such as its tool calls, the files it reads and the output of its commands. |
| **Terminal** | The agent's terminal as it runs, with everything the agent prints. It takes your typing. |
| **Split view** | Both views, with the summary above the terminal. Drag the divider between them to resize the summary. |

Use the summary to follow several agents and to answer what waits on you. Use the terminal to watch the details of a turn, or to type to the agent directly.

Press <kbd>Ctrl+&#96;</kbd> or <kbd>⌘⇧T</kbd> to switch between the summary and the terminal. On Linux, the second shortcut is <kbd>Ctrl+⇧T</kbd>. Each session remembers the view you used last. To choose the view a session opens in, set **Open a session in** in *Settings › Terminal*. An agent that you just started opens in its terminal, which **Open an agent you start in** changes.

A session in your own terminal has no terminal to show. Its Terminal view says so, and offers to resume the session in Pinrail once it has stopped.

## How Pinrail knows the session

The `pinrail` command reads the agent and its session from the agent's environment, such as `CLAUDE_CODE_SESSION_ID` for Claude Code, and sends them with each review, question and status. The agent does nothing to name its session. A review or a question sent from no agent, such as from a CI job, belongs to no session and shows only in the inbox.

A session that Pinrail starts shows at once. A session in your own terminal shows when its agent first sends a question, a review or a status, works on a work item, or waits for you in its terminal.

## What the agent is doing

Pinrail learns what an agent is doing from two sources.

- **The agent's hooks.** Turn on **Report status** for the agent in [Settings › Agents](/docs/using/settings/#agents), and Pinrail installs hooks that report each turn. This works for a session in your own terminal too.
- **The agent's terminal.** Some agents, such as Claude Code, report their state in the terminal they run in. Pinrail reads these reports for a session that it runs, also while **Report status** is off.

A session with neither source shows no state of its own. It shows only what waits on you in it.

![A diagram of the states of a session. Your prompt moves an idle session to Working. When the agent asks for a permission, an answer, a question or a review, the session shows Needs you until you answer. When the turn ends, the session shows Finished, and it becomes Idle once you open it. From any state, the session is Stopped when the agent exits or you stop it, and resuming it makes it Idle again.](diagram:session-states "The states of a session, and what moves it between them")

| State | What it means |
|---|---|
| **Needs you** | A question or a review of the session waits for you, or the agent waits for a permission or an answer in its terminal. |
| **Finished** | The agent finished a turn since you last looked. The session counts as seen once you have had it open for a second. |
| **Working** | The agent works on a turn. |
| **Idle** | The agent waits for your next prompt. |
| **Stopped** | The agent exited, or you stopped it. |

![A session's page with the sidebar grouped by status. Needs you, Finished, Working, Idle and Stopped each list their sessions, with a mark on each agent's icon. A chip beside the open session's title says that the agent has worked for eight minutes.](screenshot:session-states "Sessions under their states in the sidebar, and the chip beside a session's title.")

The sidebar and the Sessions page mark each session with its state, and a chip beside the session's title says what it waits for or how long the agent has worked. What an agent can report depends on the agent: some do not report that they wait for a permission or an answer. A session that reports work and then nothing for 30 minutes shows **Status unknown**.

To see a finished session as finished again, choose **Mark as unread** in the **⋯** menu beside its title.

## What a session sends you

An agent sends these with the `pinrail` command, whichever way its session runs.

| What | Sent with | Waits for you | What the agent gets back |
|---|---|---|---|
| **Questions** | `pinrail ask` | Yes, in the inbox | Your answers, your notes and your comment |
| **Reviews** | `pinrail submit` | Yes, in the inbox | Your decision, as the plugin defines it |
| **Statuses** | `pinrail status` | No | Nothing |
| **Notes** | `pinrail status --note` | No | Nothing |

Questions and reviews ask for your answer, and the agent waits until you give it. Statuses and notes only tell you something.

![A session's page. Its header names the stage and the two work items the session holds. Its thread shows a finished stage, the stage the agent is on, a status, a note with a chart, a question with three choices and a review, both waiting. The details beside it list what waits on you, the work items, the stages, and the session's agent, branch and folder.](screenshot:session-work "A session with what it sent: stages, a status, a note with a chart, and a question and a review that wait for you.")

### Questions

A question is a short request for an answer the agent needs before it can go on, such as a choice, a fact or a preference. A set of questions is a [review](/docs/concepts/reviews/) of its own kind: it waits in the inbox, ends in the same ways, and the command that asked it exits with the same codes. It needs no plugin, and you answer it in the session's thread rather than in a plugin's view.

The questions an agent asks together are one set, and you send their answers together. You can answer in your own words, add a note to any answer, and add a comment on the whole set. A question you leave alone goes back as not answered, which is not a decision. Declining the set tells the agent to stop, as discarding a review does.

### Statuses and stages

A status is one line that says what the agent is doing, such as *Running the integration tests*. It shows in the thread at the moment it was sent, and the newest status shows on the Sessions page and in the sidebar. A status names the work. It does not change the session's state.

An agent groups its work into **stages**. It starts a stage with `pinrail status --start`, which names it, and the stage becomes the session's title. It ends the stage with `pinrail status --done` once the work is approved and delivered, or when nothing needs a review. A session can have several stages, one after the other.

### Notes

A note is a message in Markdown that tells you something and asks nothing, such as a summary of a stage, a decision the agent took on its own, or what an investigation found. It can carry files, such as a chart or a log, and shows images in place.

## Messages to the agent

You write to an agent in the message box under its session's thread. How the message reaches the agent depends on where the session runs.

- **A session that Pinrail runs.** Pinrail types the message into the agent's terminal as your prompt once the agent is idle. While the agent is starting, works on a turn or waits for an answer in its terminal, the message waits, and Pinrail types the waiting messages one at a time.
- **A session in your own terminal.** The agent receives the message the next time it runs `pinrail status`.

A message that the agent has not received yet waits at the top of the message box, with a few words that say when it will go, and you can delete it there. A message that has waited a minute for the agent to start shows **Type now**, which types it at once. The message joins the thread when the agent receives it. A message that you write while the agent is stopped is typed once you resume the session in Pinrail.

![The message box under a session's thread, with two messages waiting at its top. Each says that it goes to the agent with its next status, and has a button that deletes it.](screenshot:session-messages "Two messages wait for an agent in its own terminal, until it next reports a status.")

To attach files to a message, paste them, drop them on the message box or pick them with its attach button. The agent receives each file's path after your text.

## Sessions and work items

A session can hold [work items](/docs/concepts/work-items/). An agent that you start on a work item gets the item's title, description and comments as its task, and its session claims the item. While a session holds items, the reviews and questions it sends belong to them, and its thread shows the changes it makes to items and its comments on them. A comment you add to an item the session holds reaches the agent the next time it runs `pinrail status`.

## The life of a session

### Names

Every session has a name of its own, made of an adjective and a chemical element, such as **Careful Calcium**. The session shows under this name until its agent names a stage, which becomes its title, and the session's details always show the name. Click the name in the session's header to change it. A name that you give the session shows as its title, with the stage under it.

### Resuming

**Resume** in the header of a stopped session starts its agent again in Pinrail, on the same conversation. This also works for a session that you started in your own terminal. When the session's folder was moved or deleted, Pinrail offers **Choose another folder…**. After the computer restarts, the Sessions page lists the agents that the restart stopped, with **Resume all**.

### A new session in the same terminal

An agent can go on in a new session without leaving its terminal, as Claude Code does after `/clear`. The open session page then follows the terminal to the new session, and the session it left links to the new one.

### Archiving

Archiving a session removes it from the Sessions page and the sidebar and deletes nothing. Its questions and reviews stay in the inbox while they wait. When Pinrail runs the session's agent, archiving stops the agent. An archived session comes back by itself when its agent sends something again.

### Deleting

**Delete…** in a session's menu removes the session with its statuses, notes and messages. Pinrail stops the agent first if it runs it, withdraws the questions and reviews that still wait, and gives up the work items the session holds. Decided reviews stay in the inbox's History.

:::caution[Deleting a session cannot be undone]
Archive a session to remove it from the lists and keep what it sent. Delete it only when you no longer need its thread.
:::

When you set *Keep reviews for* in *Settings › Data*, Pinrail deletes old reviews from the history, and with them each session that has no review left and no activity in that time, unless Pinrail still runs its agent. Deleting a session releases the work items it held.

## Where to go next

- [Questions and sessions](/docs/using/sessions/): answering questions, the thread, and the keys to use.
- [The CLI](/docs/agents/cli/): `pinrail ask` and `pinrail status`, with every option.
- [Reviews](/docs/concepts/reviews/): what a review is, and how it ends.
- [Work items](/docs/concepts/work-items/): the tasks that sessions hold.
