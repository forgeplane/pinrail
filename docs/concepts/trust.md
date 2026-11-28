---
title: What runs where
description: "What stays on your machine, who can act as you, what a plugin's view can and cannot do, and what installing a plugin runs."
---

Pinrail runs your agents and sits between them and the things they do, so it matters what it can reach. This page lists what runs where, and what each part is allowed to do.

## Everything is on your machine

![A diagram of what runs on your computer. The Pinrail app has a window and a core. The window carries your token, shows plugin views in a sandbox, and is the only part that acts as you. The core serves its API on loopback only and keeps the data directory, with your sessions, work items, reviews and their files. The core reaches the terminal host and its holder over a local socket, with a secret, and they run the agents you start. Agents and other programs use the pinrail command, which reads and submits but never acts as you. Anonymous usage data and crash reports, update checks, and feedback that you send leave your computer, and you can turn off the first two.](diagram:trust-boundary "What runs on your computer, what acts as you, and what leaves it")

- **The server** listens on your machine's loopback address, `127.0.0.1`, port `4747` by default. Programs on your computer can reach it, but nothing on the network can. Web pages you visit cannot reach it either. It answers only requests addressed to `127.0.0.1` or `localhost`. It changes nothing unless the request is JSON, and browsers do not let another site send JSON to it.
- **Your sessions, work items, reviews and decisions** are stored in your data directory, `~/.local/share/pinrail` by default (`%LOCALAPPDATA%\dev.pinrail.desktop` on Windows), with the files sent beside reviews. Pinrail makes that directory readable only by your user account, so other users of the computer cannot read them. There is no account and no cloud service.
- **The app** shows your sessions, work items and reviews, and sends your decisions. No review, decision or file leaves your machine unless an agent, acting on your decision, sends it. Pinrail sends anonymous usage data and crash reports, which hold none of these, and you can turn them off. See [Usage data and crash reports](/docs/using/privacy/).
- **The terminal host** is a separate program on your computer that runs the agents you start in Pinrail. It listens on a socket that only your user account can reach, and accepts a connection only with a secret that it keeps in a file only you can read. See [The terminal host](/docs/concepts/terminal-host/).

## Who can act as you

Only the Pinrail app can act as you. Each time the app starts, it makes a new token, keeps it in memory and gives it to its own window. The server takes an action as yours only when the request carries that token, so another program on your computer cannot decide a review, message an agent or change a setting in your name.

| | The app's window | The `pinrail` command and other programs |
|---|---|---|
| **Read** reviews, sessions and work items | Yes | Yes |
| **Submit** a review, ask a question, send a status, and add, take or close a work item, as an agent | Not its job | Yes |
| **Decide or discard** a review, or **change a work item**, as you | Yes | No. `pinrail decide` records that the decision was made with the command, and by which agent. |
| **Send an agent a message**, and start, resume or stop an agent | Yes | No |
| **Change** settings, projects, themes and sessions | Yes | No |

The server trusts the programs that you run to read what it holds and to send it work, as an agent does. It does not let them speak for you.

:::caution[A setting that lets the command act as you]
The commands `pinrail start`, `pinrail stop`, `pinrail attach` and `pinrail output` act as you, so they need the token. They work only while the setting [`runtime.cli_control`](/docs/reference/settings/) is on. Pinrail then writes the token to the file `person-token` in your data directory, readable only by your user account, and removes it when the app quits. Any program that you run, including an agent, can read that file and then act as you. The setting is off by default.
:::

## What a plugin's view can do

A plugin's view is a page the app shows inside a sandboxed frame. However the plugin was installed, the view:

- **can** draw, and exchange [messages](/docs/building/protocol/) with the app;
- **can** ask the app to open a link in your browser or mail client. The app asks you first, showing the site, unless you allowed that site for this plugin. It never opens an address on your own computer, and it always asks about an email or a very long address;
- **can** ask the app to copy text, or an image its review carries, to the clipboard, but only when the plugin's manifest declares the `clipboard` capability. The app shows you a notice for every copy;
- **cannot** use the network, so it cannot make requests or load web fonts, scripts or styles from elsewhere;
- **cannot** store data, and can read only what the app sends it. When the view asks for one of its review's attachments by name, the app sends it, but the view cannot read another review's attachments;
- **cannot** see other reviews, other plugins, or your files.

Everything a view shows arrives in the review's payload or in its attachments. That is why a code review sends the diff rather than a link to it.

Attachments are only displayed inside a view's sandbox, whatever they contain. The view asks the app for an attachment's contents and displays it itself. The artifact plugin, for example, displays an attached HTML page this way. The app itself never opens an attachment. It lists each one by name and size, and saving one writes its contents to disk unchanged.

## What installing a plugin runs

Installing a plugin runs nothing. Pinrail installs a plugin from a folder or a zip on your computer: it copies the plugin's static files into its store and checks them, and it downloads nothing. A plugin whose view is made with a build tool is built before it is installed, by its author or by you.

:::caution[Building a plugin runs code]
If you build a plugin yourself, for example with `npm ci && npm run build`, that build runs on your machine as you: `npm ci` runs the install scripts of every package in the dependency tree, and the build runs whatever the plugin's package says. Plugins and their zips are not signed, and plugin authors are not vetted, so install plugins only from people whose code you would run.
:::

After installation, a plugin consists only of static files, and its view runs in the same sandbox as any other.

A plugin can also ship a skill: instructions for your agents, and sometimes scripts they run. Pinrail runs none of it itself, but your agents follow those instructions, and run those scripts, as you. So the skill of a plugin copied from a folder or a zip reaches your agents only after you allow it, and Pinrail writes its scripts without permission to run them directly. Read a plugin's `skill/SKILL.md` before you allow it. See [Plugins with a skill](/docs/using/installing-plugins/#plugins-with-a-skill).

## What the agent can do

An agent that you start in Pinrail runs as you, with your files and your permissions, as it does in your own terminal. Pinrail does not put the agent in a sandbox or limit what it does. The agent's own permission checks still apply. If you add arguments that turn them off, such as `--dangerously-skip-permissions`, in *Settings › Agents*, the agent's card there warns you.

An agent can run any `pinrail` command. It can therefore read your reviews and work items, send you questions and reviews, and add and close work items as itself. It cannot act as you, as [Who can act as you](#who-can-act-as-you) describes.

Pinrail gives an agent a way to ask, and reports your answer faithfully. Whether an agent asks, and whether it respects the answer, is up to its instructions and its harness. Write instructions that name the moments to ask, and prefer tools that enforce them. See [Instructing an agent](/docs/agents/instructing/).
