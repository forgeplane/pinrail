---
title: The terminal host
description: "How Pinrail runs agents in a terminal host and a holder, and why they keep running when the app quits, updates or crashes."
---

Pinrail does not run your agents inside the app. It runs them in a **terminal host**, a separate program that the app starts and then only connects to. An agent therefore keeps working when you quit Pinrail, when the app updates, and when the app crashes. This page explains how that works, and what to expect in each case.

:::note[Agents keep running when you quit Pinrail]
When you quit Pinrail while agents run in it, Pinrail asks whether to leave them running or to stop them first. The agents that you leave running carry on with their work, and you can open them again when you reopen Pinrail. To make Pinrail stop asking, choose what it does under **Quitting with agents running** in *Settings › General*.
:::

## The three processes

Three programs take part in running an agent. Each one owns a different part of the work, so that a failure in one of them does not end your agents.

![A diagram of the processes that run an agent. The Pinrail app, with its window and core, connects to the terminal host over a local socket. The terminal host keeps each terminal's screen, its viewers and the typing. It asks the holder for an agent, and the holder shares the agent's terminal with it. The holder starts the agents as their parent, each in its own terminal. The host reads each terminal's output and types the input. The host, the holder and the agents keep running without the app.](diagram:terminal-host "The app, the terminal host, the holder and the agents")

| Process | What it owns | When it ends |
|---|---|---|
| **The app** | The window, and the core behind it, which keeps your sessions, work items and reviews. The app starts the host when none runs, and asks it to start, show and stop agents. | When you quit Pinrail. |
| **The terminal host** | The terminals. For each one, it reads what the agent prints, keeps the screen and its history, sends them to the views that show the terminal, and types what you type. | When Pinrail stops it, or after 10 minutes with no terminal and no app connected. |
| **The holder** | The agents themselves. It starts each agent as its child, in a terminal that it opens, and keeps that terminal open. | 30 seconds after its last agent has ended and no host is connected. |

The app does not start the host as its own child. It starts it through `launchd` on macOS and as a unit of your `systemd` user manager on Linux, so that the system does not end the host with the app. The host runs from a copy of its program in Pinrail's data directory, which an update of the app does not replace.

## The holder

The holder is what makes the design dependable. It does three things, and nothing else:

1. It opens a terminal for each agent and starts the agent in it, as the agent's parent process.
2. It keeps open the terminal and a buffer of the terminal's recent output, the last 1 MiB.
3. It shares both with the host, and tells the host when an agent exits.

The holder never reads the terminal or the buffer. It does not interpret what an agent prints, draw a screen or handle your typing. All of that is the host's work, which is where a bug is far more likely. A program that only keeps things open has little reason to crash. This matters because the agents depend on it: if a holder ended, the terminals it keeps would close, and its agents would end with them.

The way the host talks to the holder is also frozen. It has four messages, and a new release of Pinrail does not change them. A new host can therefore always talk to a holder that an older release started, however long its agents have been running.

A holder also looks after its agents when no host is there. An agent that prints to a terminal that nobody reads would stop and wait. So a holder that still has agents, and has had no host for 5 seconds, starts a host itself.

## What keeps an agent running

| What happens | The host | The holder and its agents |
|---|---|---|
| You quit Pinrail and leave the agents running | Keeps running | Keep running |
| The app crashes | Keeps running | Keep running |
| The app updates | Replaced by the new release's host | Keep running |
| The host crashes | Started again | Keep running |
| The computer restarts | Ends | End |

### Quitting and reopening Pinrail

When you quit Pinrail with agents running, a dialog lists them, with **Quit**, which leaves them running, and **Stop agents and quit**. **Quitting with agents running** in [Settings › General](/docs/using/settings/#general) sets the answer, so that Pinrail no longer asks.

When you open Pinrail again, the app finds the host that is still running and connects to it. Each session shows its terminal as the agent left it, with what the agent printed while the app was closed.

:::caution[When macOS quits Pinrail for you]
macOS can quit Pinrail without the dialog, for example from the Dock or when you log out. Pinrail then stops the agents only if **Quitting with agents running** is set to *Stop them*.
:::

### Updating Pinrail

Restarting Pinrail to install an update does not ask about the agents, because they keep running through it.

After the update, the app starts the host of the new release. The old host hands each terminal's screen over to the new one and exits, and the holder and its agents are not touched. An agent that you start after the update runs under a holder of the new release. The older holder keeps the agents it has, and exits when the last of them ends.

### A host that crashes

When the host ends without warning, the holder still has every terminal open, and the agents keep running. The holder, or the app if it is open, starts a new host within seconds. The new host takes the terminals back from the holder, rebuilds each screen from the buffer of recent output, and asks each agent to draw its screen again.

### Restarting the computer

A restart ends the host, the holder and every agent. When you open Pinrail again, the Sessions page lists the agents that the restart stopped, with **Resume all**, which resumes them one by one. Each agent continues its conversation as it does after any [resume](/docs/concepts/sessions/#resuming).

## Viewing and typing in a terminal

A terminal can have several viewers at once: the **Terminal** view of its [session](/docs/concepts/sessions/) in the app, and every terminal in which you run `pinrail attach`. Each viewer first receives the terminal's screen, and then its output as the agent prints it.

One viewer at a time controls the terminal. Its typing goes to the agent, and the terminal takes its size. The other viewers only watch. Click the terminal in the app, or press a key in `pinrail attach`, to take control.

The host keeps the output of a terminal for you to read later:

- Each terminal keeps 10,000 lines of history.
- When an agent ends, the host keeps its last screen for up to 24 hours. The session's Terminal view shows it, with how the agent ended.
- `pinrail output <run>` prints the last lines that a terminal showed, and with `--wait-for` it waits until they contain a given text. See [`pinrail output`](/docs/reference/cli/#pinrail-output).

A host runs at most 64 terminals at once.

## Where the host keeps its files

The host's files are in the `host` folder of Pinrail's data directory, which is `~/.local/share/pinrail` by default.

| File | What it holds |
|---|---|
| `host.json` | The host's registration: its process, its build, the path of its socket and a secret. Only your user account can read it. |
| `terminals.json` | A record of each terminal that runs or ended recently. |
| `host.log` | The host's log. |
| `bin/` | The copies of the host's program, one for each build that still has a host or a holder running. |

The app finds the host through `host.json`. It connects to the socket, which is in your user's runtime directory, and presents the secret. The host accepts no connection without it. To see the host's state and its terminals, run `pinrail host status`.

## Pinrail Host in macOS privacy prompts

On macOS, the host and the holder run as an app of their own, **Pinrail Host**. Every agent is a child of the holder, so macOS names Pinrail Host, and not Pinrail or your terminal, when an agent asks for something that macOS protects, such as the files in your Documents folder. A permission that you grant is granted to Pinrail Host.
