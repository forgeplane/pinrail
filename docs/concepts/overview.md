---
title: Overview
description: "The parts of Pinrail, how they connect, and how agents run in it and report to it."
---

Pinrail is a desktop app for working with coding agents. It runs them, tracks their work items, and brings everything that needs you, such as questions, permissions and reviews, into one inbox. This page introduces the parts of Pinrail and shows how they connect. The other pages under Concepts describe each part in detail.

![A diagram of you, Pinrail and your agents. You start agents, send them messages and decide in Pinrail. Pinrail holds the inbox, the sessions and the work items, and runs the agents in its terminal. The agents report and ask through their hooks and the pinrail command. Pinrail shows you what needs you, such as questions, reviews and permissions.](diagram:how-pinrail-works "You, Pinrail and your agents")

## The parts

| Part          | What it is                                                                                                                                                                                       |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Agent**     | A coding agent, such as Claude Code, Codex or OpenCode. Pinrail works with any agent that can run a command.                                                                                     |
| **Session**   | One run of an agent, such as a Claude Code conversation. Pinrail shows each session's state and a thread of what the agent sent you. See [Sessions](/docs/concepts/sessions/).                   |
| **Work item** | A task that you and your agents share, with sub-items and dependencies. An agent takes the next item that is ready and records its progress on it. See [Work items](/docs/concepts/work-items/). |
| **Inbox**     | The list of notifications. It shows what waits on you first, and the updates from your agents below it. See [The inbox](/docs/using/inbox/).                                                     |
| **Question**  | A short request for an answer that an agent needs before it can go on. You answer it in the session's thread. See [Questions](/docs/concepts/sessions/#questions).                               |
| **Review**    | A request for your decision on work that an agent is about to do, shown in a view made for that work. See [Reviews](/docs/concepts/reviews/).                                                    |
| **Plugin**    | A kind of review. It defines the data an agent sends, the view you decide in, and the decision that goes back. See [Plugins](/docs/concepts/plugins/).                                           |
| **Project**   | A set of repositories that you group under one name. You can filter the inbox and the Sessions page by project. See [Projects](/docs/using/projects/).                                           |

## Where an agent runs

An agent runs in Pinrail or in your own terminal. Pinrail shows its session in both cases.

- **In Pinrail.** You start the agent from the **New session** page or from a work item's panel. Pinrail runs it in a terminal of its own, which you can watch and type into. You can send the agent messages from its session, and Pinrail types each one into the terminal once the agent is idle. You can also stop the agent and resume it later. Pinrail starts and resumes Claude Code, Codex, OpenCode, Copilot CLI, Cursor, Grok CLI, Pi and Antigravity CLI.
- **In your own terminal.** You start the agent as usual. Pinrail shows the session from what the agent reports, and you answer its questions and reviews in the app. A message that you send from the session reaches the agent the next time it reports a status.

:::note[Agents keep running when you quit Pinrail]
When you quit Pinrail while agents run in it, Pinrail asks whether to leave them running or to stop them first. The agents that you leave running carry on with their work, and you can open them again when you reopen Pinrail. To make Pinrail stop asking, choose what it does under **Quitting with agents running** in _Settings › General_.
:::

## How an agent reports

An agent tells Pinrail what it is doing in two ways.

- **Its hooks.** Pinrail installs them when you connect the agent in [Settings › Agents](/docs/using/settings/#agents). They report the agent's turns, your prompts and its replies, and whether the agent is working or has finished. Depending on the agent, they also report that it waits for a permission or an answer.
- **The `pinrail` command.** The agent runs it to send statuses and notes, to ask questions, to submit reviews and wait for your decision, and to add, take and close work items. Pinrail's skill, which connecting the agent also adds, tells the agent how to use the command. See [The CLI](/docs/agents/cli/).

## What needs you

Three things wait on you. The inbox lists them under **Waiting on you**, and each one also shows in its session.

- A **question** waits for your answer, which you give in the session's thread.
- A **review** waits for your decision, which you make in the plugin's view.
- An **agent that waits for a permission or an answer** in its terminal waits until you answer it there.

An agent that asked a question or submitted a review waits, and receives your answer as soon as you give it. A review's decision is structured. For a code review, for example, it lists the findings that you accepted and the ones that you rejected, with your notes, so that the agent can act on each of them.

Everything else is an update, such as an agent that finished its turn, a work item that an agent added or closed, or a comment on a work item. The inbox lists the updates under **Updates**, below what waits on you.

## Where your data stays

Pinrail runs on your computer. It keeps your sessions, work items, reviews and decisions in a data directory on your computer, and its server accepts connections from your computer only. Pinrail sends anonymous usage data and crash reports, which contain none of your work, and you can turn them off. See [What runs where](/docs/concepts/trust/) and [Usage data and crash reports](/docs/using/privacy/).
