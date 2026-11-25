---
title: Pinrail
description: "Pinrail is a desktop app for working with coding agents. It runs them, tracks their work items, and brings everything that needs you into one inbox."
---

Pinrail is a desktop app for working with coding agents. It runs them, tracks their work items, and brings everything that needs you, such as questions, permissions and reviews, into one inbox.

![A two-minute demo of Pinrail, with captions](video:demo "Pinrail in two minutes: an agent asks, and each review is decided in the app.")

![The Pinrail inbox, with eleven reviews waiting from different agents and projects.](screenshot:inbox)

## What Pinrail does

- **It runs your agents.** Start Claude Code, Codex, OpenCode and other agents from Pinrail. Each agent runs in a terminal that you can watch and type into. You can send the agent messages, stop it and resume it later.
- **It shows what each agent is doing.** Each session shows whether its agent is working, needs you or has finished its turn, with a thread of the agent's questions, reviews, statuses and notes.
- **It tracks the work.** You and your agents share work items, which have sub-items and dependencies. An agent takes the next item that is ready, and you can start an agent on an item from the item's panel.
- **It brings what needs you into one inbox.** Questions, reviews and agents that wait for a permission or an answer come first. The updates from your agents follow.
- **It shows each review in a view made for the work.** You review a diff, a draft or a set of images in a view built for that content, and the agent gets back a structured decision. See [Reviews in a view made for the work](#reviews-in-a-view-made-for-the-work).
- **It keeps your data on your computer.** Pinrail stores your sessions, work items and reviews on your computer, and its server accepts connections from your computer only. It sends anonymous [usage data and crash reports](/docs/using/privacy/), which you can turn off.

:::note[Agents keep running when you quit Pinrail]
When you quit Pinrail while agents run in it, Pinrail asks whether to leave them running or to stop them first. To make Pinrail stop asking, choose what it does under **Quitting with agents running** in *Settings › General*.
:::

## Reviews in a view made for the work

Without Pinrail, an agent that needs your approval asks in its terminal and gets back a "yes" or a paragraph of text. It cannot tell which part you approved and which part you want changed. In Pinrail, the agent submits the work for review, and you review it in a view made for that kind of work:

- A **code review** shows the diff with the agent's findings on its lines. You accept, reject or edit each finding.
- A **document**, such as a plan or a spec, is shown in full. You comment on each section.
- A set of **images** is shown side by side. You choose one and draw a box on the part that should change.

![The code review plugin: a TypeScript diff with one finding accepted and a note to the agent being written, another rejected with its reason.](screenshot:review "A code review: one finding accepted with a note to the agent, another rejected with its reason.")

When you finish, the agent receives a structured decision: what you accepted, what you rejected, and your note on each part, checked against the plugin's schema. The agent acts on each part, and when you ask for changes it submits a new round, which Pinrail shows beside your earlier comments.

Each kind of review is a [plugin](/docs/concepts/plugins/). Pinrail comes with plugins for code reviews, lists, questions, documents and images, and [more plugins](/docs/plugins/more/) cover emails, web pages, audio, video and other content.

:::tip[Build a view for your own work]
A plugin is a manifest, two JSON Schemas and an HTML page, so a view for a new kind of work is quick to make. Your agent can write the plugin for you: `pinrail plugins new` creates one, and [Writing a plugin](/docs/building/writing/) describes the rest.
:::

## Where to start

| If you want to… | Read |
|---|---|
| Install Pinrail and set it up | [Install](/docs/getting-started/install/) |
| Understand the parts of Pinrail and how they connect | [Overview](/docs/concepts/overview/) |
| Follow your agents' sessions and answer their questions | [Questions and sessions](/docs/using/sessions/) |
| Plan work for your agents | [Work items](/docs/using/work-items/) |
| Tell your agent when to ask you | [Instructing an agent](/docs/agents/instructing/) |
| Choose a plugin for what your agent does | [Plugins](/docs/plugins/) |
| Ask a person before a step in a script or a CI job | [Scripts and CI](/docs/agents/workflows/) |
| Build a plugin for your own kind of review | [Writing a plugin](/docs/building/writing/) |

## How it works

![A diagram of you, Pinrail and your agents. You start agents, send them messages and decide in Pinrail. Pinrail holds the inbox, the sessions and the work items, and runs the agents in its terminal. The agents report and ask through their hooks and the pinrail command. Pinrail shows you what needs you, such as questions, reviews and permissions.](diagram:how-pinrail-works "You, Pinrail and your agents")

You start an agent in Pinrail, which runs it in a terminal of its own, or in your own terminal as usual. Pinrail shows the agent's session in both cases, and the inbox lists what needs you first. The [Overview](/docs/concepts/overview/) describes each part.

An agent tells Pinrail what it is doing through its hooks and through the `pinrail` command. Pinrail installs the hooks when you connect the agent in [Settings › Agents](/docs/using/settings/#agents). The agent runs the command to send statuses and notes, to ask questions, to submit reviews and wait for your decision, and to add, take and close work items.

The agent's own instructions say which steps need you. Each kind of review is a [plugin](/docs/concepts/plugins/), which defines what the agent sends, the view you decide in, and the decision that goes back.
