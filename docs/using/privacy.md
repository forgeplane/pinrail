---
title: Usage data and crash reports
description: "What Pinrail sends to help improve it, what it never sends, and how to turn it off or see what is sent."
---

Everything you review stays on your computer. To learn which features are used and to fix crashes, Pinrail sends anonymous usage data and crash reports to its own server. Both are on by default, and you can turn either off at any time in *Settings › Privacy*.

Nothing is recorded or sent until you have answered the notice about it. The notice appears at the top of the inbox the first time Pinrail starts. A Pinrail server that runs without the app's window, such as one started with `pinrail serve`, never shows the notice, so it records nothing.

## What usage data contains

Usage data is a list of events. Each event has a name, the time it happened, and a few fields that hold numbers, true or false, or names that Pinrail's code fixes.

| Event | When it is recorded | Fields |
|---|---|---|
| `app_started` | Pinrail starts, or you answer the notice the first time | whether it is the first start |
| `review_decided` | you decide a review | the plugin, whether you approved it or asked for changes, the number of rounds, and how many seconds it waited |
| `questions_answered` | you answer an agent's questions | the number of questions and of those left unanswered, and whether you sent the reply the agent suggested |
| `review_discarded` | you discard a review | the plugin |
| `session_started` | an agent reports its first status in a session | the agent |
| `item_changed` | a work item is added, claimed, held, queued, done or cancelled | the change, and whether an agent made it |
| `plugin_installed` | a plugin is installed, updated or removed | the plugin, whether it is an official plugin, and whether it was removed |
| `feature_used` | you use a feature, counted once a day for each | the feature, such as `command_palette` or `page_items` |
| `cli_used` | an agent or a script runs a `pinrail` command, counted once a day for each command and agent | the command, such as `ask`, and the agent |

A plugin and an agent are sent by name when the name is at most 40 letters, digits, `-`, `_` and `.`, and as `other` otherwise.

Every batch of events also carries the version of Pinrail, the operating system and its version, the processor architecture, and the release channel.

Usage data never contains review titles or payloads, decisions, notes, comments, questions or answers, work item titles or descriptions, file names or paths, repository or project names, session ids, what agents write, your user name, or your computer's name.

## What a crash report contains

When Pinrail crashes, or the app's window has an error that nothing handled, Pinrail keeps a report of it in its data directory: where it happened, the error message and the stack trace. Before the report is written to disk, Pinrail removes what could identify you or your work. Each file path becomes its file name, and quoted text, web addresses, email addresses and anything that looks like a key or a token become `…`.

Pinrail sends the report the next time it starts, then deletes it. Only one report waits at a time, the first since the last one was sent.

## How it is identified and kept

- **An install id**: 32 random characters that Pinrail creates the first time it records an event, and keeps in its data directory. Nothing ties it to you or to your computer. Turning usage data off deletes it, and turning it on again creates a new one. A crash report carries it only while usage data is on.
- **A session id**: a new random id each time Pinrail starts, which groups the events of one run.

Pinrail sends the events in batches to `api.pinrail.dev`, every 5 minutes or as soon as 50 are waiting. A batch that cannot be sent is tried once more and then dropped. Quitting Pinrail never waits for a send.

The server does not store your IP address. It keeps events and crash reports for 13 months, and then deletes them. Pinrail does not share them with anyone.

## Turning it off

Any of these turns it off:

- In *Settings › Privacy*, turn off **Send usage data**, **Send crash reports**, or both.
- Set the environment variable `DO_NOT_TRACK` or `PINRAIL_TELEMETRY` to any value before Pinrail starts. Both switches are then off, and *Settings › Privacy* says which variable turned them off.
- Set `telemetry.usage` or `telemetry.crashes` to `false` in `settings.json` in the data directory. See [Where settings are stored](/docs/using/settings/#where-settings-are-stored).

When usage data is off, nothing is recorded or sent, and the install id and the events that were waiting are deleted. When crash reports are off, no report is kept, and one that was waiting is deleted. Data that was already sent stays on the server until it is deleted after 13 months.

## Seeing what is sent

In *Settings › Privacy*, **Show what is sent** shows the install id, the events waiting to be sent, the last 100 sent, and the crash report waiting, if any, as JSON.

To watch the batches instead of sending them, start Pinrail with `PINRAIL_TELEMETRY_DEBUG=1`. Each batch is then printed to standard error and not sent.

## Other connections

Pinrail connects to the internet in two other cases:

- **Updates**: while *Check automatically* is on, Pinrail looks for a new version on GitHub when it starts and every few hours. See [About](/docs/using/settings/#about).
- **Send feedback**: what you write in *Send feedback*, the files you attach, and the diagnostics unless you leave them out, are sent to `api.pinrail.dev` only when you send them, with the version of Pinrail and of the operating system.
