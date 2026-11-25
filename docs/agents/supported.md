---
title: Supported agents
description: "What Pinrail does with each coding agent: its skill, its status reports, and whether Pinrail starts, resumes and messages it."
---

Pinrail works with any agent that can run the `pinrail` command. For the agents on this page it does more: it installs its skill, shows what the agent is doing, and starts, resumes and messages the agent. The tables show what each agent supports, and the sections under them say what Pinrail installs for each one.

In the tables, **✓** means yes, **—** means no, and **◐** means partly, with a note under the table.

## What Pinrail shows

| Agent | Skill | Report status | Working | Permission | Answer | Finished |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| **Claude Code** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Codex** | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| **OpenCode** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Copilot CLI** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Cursor** | ✓ | ✓ | ✓ | ◐ | — | ✓ |
| **Grok CLI** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Pi** | ✓ | ✓ | ✓ | ◐ | ✓ | ✓ |
| **Antigravity CLI** | ✓ | ✓ | ✓ | ◐ | ◐ | ✓ |

- **Skill** is Pinrail's skill, which tells the agent how to use the `pinrail` command. You add it in [Settings › Agents](/docs/using/settings/#agents).
- **Report status** installs what reports the agent's state: hooks, an extension or a plugin, depending on the agent. You turn it on in the same place.
- **Working**, **Permission**, **Answer** and **Finished** are the states that the agent reports: that it works on a turn, waits for a permission, waits for your answer to a question it asks in its terminal, and has finished its turn. See [What the agent is doing](/docs/concepts/sessions/#what-the-agent-is-doing).
- **Cursor** has no hook for a permission. Pinrail shows one only in a session that it runs, and only while you are not looking at that session's terminal.
- **Pi** has no permission checks of its own. Pinrail shows a confirmation that one of Pi's extensions asks for.
- **Antigravity CLI** has no hook for a permission or a question. Pinrail shows them only in a session that it runs, from what the agent reports in its terminal.

## What Pinrail controls

| Agent | Start | Resume | Messages | Open in Pinrail | Attached files |
|---|:-:|:-:|:-:|:-:|:-:|
| **Claude Code** | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Codex** | ◐ | ✓ | ✓ | — | — |
| **OpenCode** | ✓ | ✓ | ✓ | — | ◐ |
| **Copilot CLI** | ✓ | ✓ | ✓ | — | — |
| **Cursor** | ✓ | ✓ | ✓ | — | — |
| **Grok CLI** | ✓ | ✓ | ✓ | — | — |
| **Pi** | ✓ | ✓ | ✓ | — | — |
| **Antigravity CLI** | ◐ | ✓ | ✓ | — | — |

- **Start** and **Resume** run the agent in Pinrail's [terminal host](/docs/concepts/terminal-host/). **Codex** and **Antigravity CLI** choose their own session id, which Pinrail learns from their hooks, so starting them needs **Report status** on. The **New session** page turns it on with one click.
- **Messages** are the messages you send from a session, which Pinrail types into the agent's terminal once the agent is idle. See [Messages to the agent](/docs/concepts/sessions/#messages-to-the-agent).
- **Open in Pinrail** opens a session that still runs in your own terminal. Only Claude Code lets two processes work on one session. Any of these agents can be resumed in Pinrail once its session has stopped.
- **Attached files** means that the agent reads the files you attach to a message without asking for a permission. **OpenCode** does so through Pinrail's plugin, so it needs **Report status** on. The other agents ask, or not, as their own permission settings say.

## Agents that get the skill only

Pinrail installs its skill for these agents too. They can ask you questions, submit reviews and use work items. Pinrail does not show their state, and does not start or resume them.

| Agent | Skill folder | Sessions |
|---|---|:-:|
| **Amp** | `~/.config/amp/skills` | ✓ |
| **Augment CLI** | `~/.augment/skills` | — |
| **Cline** | `~/.cline/skills` | — |
| **Goose** | `~/.config/goose/skills` | ✓ |
| **Hermes Agent** | `~/.hermes/skills` | ✓ |
| **Kiro CLI** | `~/.kiro/skills` | ✓ |

**Sessions** means that the agent names its session to the `pinrail` command, so that what it sends shows as one [session](/docs/concepts/sessions/). What the other agents send shows in the inbox, under no session.

## Each agent

The paths are the agents' defaults. Where an agent moves its configuration folder with a variable, such as `CLAUDE_CONFIG_DIR`, Pinrail follows it. Pinrail adds its own entries beside yours and changes nothing else in a file. The versions are the ones Pinrail was tested with, not minimum versions.

### Claude Code

- **Skill:** `~/.claude/skills/pinrail`.
- **Report status** adds hooks to `~/.claude/settings.json`, or to the `settings.json` in `$CLAUDE_CONFIG_DIR`.
- Claude Code also reports its state in its terminal. A session that Pinrail runs therefore shows Working, Needs you and Finished with **Report status** off.

### Codex

- **Skill:** `~/.codex/skills/pinrail`. With it, Pinrail writes `~/.codex/rules/pinrail.rules`, a rule that runs the `pinrail` command outside Codex's sandbox, so that the command reaches the app. Codex applies the rule to a plain command, and not to one in a pipe or a heredoc.
- **Report status** adds hooks to `~/.codex/hooks.json`, or to the file in `$CODEX_HOME`, and trusts them in Codex, so that Codex does not ask you to.
- Codex reports no question that it asks in its terminal.
- Starting Codex from Pinrail needs **Report status** on.

### OpenCode

- **Skill:** `~/.config/opencode/skills/pinrail`.
- **Report status** installs a plugin, `~/.config/opencode/pinrail-opencode/tui.js`, and adds it to the `plugins` list in `~/.config/opencode/cli.json`. Both follow `$XDG_CONFIG_HOME`.
- The plugin also lets OpenCode read the files attached to a session without asking.
- Tested with OpenCode 2.0.20.

### Copilot CLI

- **Skill:** `~/.copilot/skills/pinrail`.
- **Report status** writes `~/.copilot/hooks/pinrail.json`, or the file in `$COPILOT_HOME/hooks`.
- Copilot CLI reports a session at its first prompt, not when it starts. It runs no hook when you interrupt a turn or decline a permission. In a session that Pinrail runs, the agent's terminal reports the end of such a turn.
- Tested with Copilot CLI 1.0.114.

### Cursor

- **Skill:** `~/.cursor/skills/pinrail`.
- **Report status** adds hooks to `~/.cursor/hooks.json`. They are for Cursor's CLI, `cursor-agent`.
- Cursor has no hook for a permission, and reports no question that it asks in its terminal.
- Tested with `cursor-agent` 2026.11.19.

### Grok CLI

- **Skill:** `~/.grok/skills/pinrail`.
- **Report status** writes `~/.grok/hooks/pinrail.json`, or the file in `$GROK_HOME/hooks`. Grok runs no hook in a folder that you have not trusted in Grok.
- Tested with Grok 1.0.89.

### Pi

- **Skill:** `~/.pi/agent/skills/pinrail`.
- **Report status** installs an extension, `~/.pi/agent/extensions/pinrail.ts`, or the file in `$PI_CODING_AGENT_DIR/extensions`. Pi loads it when it starts.
- Tested with Pi 1.5.2.

### Antigravity CLI

- **Skill:** `~/.gemini/config/skills/pinrail`.
- **Report status** adds a hook named `pinrail` to `~/.gemini/config/hooks.json`.
- Antigravity CLI reports a conversation at its first prompt, and reports nothing when you resume one until the next prompt.
- Starting Antigravity CLI from Pinrail needs **Report status** on.
- Tested with `agy` 1.5.12.
