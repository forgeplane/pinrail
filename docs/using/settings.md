---
title: Settings
description: "Every section of Pinrail's settings, what each option does, and where the settings are stored."
---

Open settings with <kbd>⌘,</kbd>, with the gear at the bottom of the sidebar, or from *Navigate › Settings…* in the menu. Changes take effect immediately. There is no Save button.

## General

![Settings, General: the inbox and startup.](screenshot:settings-general)

### Inbox

| Setting | What it does |
|---|---|
| **Opening a review** | *Beside the list* opens a review from the inbox or the history, or a session from the Sessions page, in a panel next to the list, which stays in view. *As a page* opens it on a page of its own. |
| **Mark read updates as done** | When the inbox marks an update you have read as done, which takes it out of the list. *On return* marks it done the next time you return to the inbox or bring its window back, and *After a day* and *After a week*, the default, mark it done that long after its last change. A question or review that waits on you is never marked done this way. **Show done** in the Filter menu lists the done updates. |

### Startup

| Setting | What it does |
|---|---|
| **Launch at login** | Start Pinrail when you log in, so agents can always ask. |
| **Closing the window** | *Hide to the menu bar* keeps Pinrail running, so reviews still arrive. *Quit* stops it. On Linux, the option is *Hide to the tray*. |
| **Quitting with agents running** | What happens to the agents that run in Pinrail when you quit it. *Ask* opens a dialog that lists them, with **Quit**, which leaves them running, and **Stop agents and quit**. Check *Do not ask again* in the dialog to make your choice the setting. *Keep them running* quits without asking, and you can open the agents again when you reopen Pinrail. *Stop them* stops every agent, then quits. |
| **Show in the menu bar** | The menu bar icon, with the number of waiting reviews and a menu to open them. On Linux, the setting is *Show in the tray*. |

## Notifications

![Settings, Notifications: system notifications, and each kind of notification with its inbox and system switches.](screenshot:settings-notifications)

### System notifications

| Setting | What it does |
|---|---|
| **System notifications** | Raise a system notification for the kinds turned on in the *System* column. |
| **Pause** | Stop system notifications for 15 minutes, an hour, or until tomorrow. Everything still reaches the inbox. |
| **Quiet hours** | Stop system notifications between two times each day, local time. The range may span midnight. |
| **Sound** | Play the system sound with each notification. |
| **System** | What your system allows, with a link to its settings when notifications are blocked. |

### Kinds of notifications

Each kind of notification has two switches. The *Inbox* switch adds it to the inbox, and the *System* switch also raises a system notification for it. Reviews, questions and an agent that waits for you are always added to the inbox. A kind that is off in the inbox never raises a system notification.

| Kind | Inbox | System |
|---|---|---|
| **Reviews**, a new review or a new round | Always | On |
| **Questions** | Always | On |
| **Agent waits**, for a permission or an answer in its terminal | Always | Off |
| **Agent finished**, or failed its turn | On | Off. It covers a stopped agent too. |
| **Agent stopped** | On | No switch |
| **Agent notes** | On | No switch |
| **Ended reviews**, withdrawn or expired | On | No switch |
| **New work items** | On | No switch |
| **Comments** on a work item | On | Off |
| **Closed work items** | On | No switch |
| **Claimed work items** | On | No switch |
| **Other changes** to a work item | Off | No switch |

The table shows the defaults.

### Muted projects and repositories

**Mute** opens the list of projects and repositories. For a muted one, only what waits for you is added to the inbox. To mute a single work item or session instead, use the bell in its header.

See [Notifications](/docs/using/notifications/) for more, including muting one plugin.

## Appearance

| Setting | What it does |
|---|---|
| **Colour scheme** | *System*, *Dark* or *Light*. Plugin views follow it. <kbd>⌘⇧L</kbd> switches it from anywhere in the app. |
| **Dark theme**, **Light theme** | The theme for each colour scheme. See [Themes](/docs/using/themes/). |
| **Custom themes** | *Open folder* opens the folder of your own themes, and *New theme…* creates one from the current theme. Any error in a theme file shows here. |
| **Text size** | *Small*, *Default* or *Large*. |
| **Tables** | How tables look in messages, notes and work item descriptions. *Bordered* draws a line around every cell, *Lines* draws lines between rows under a tinted header, and *Striped* tints every other row. |
| **Table width** | *Fit* makes each table as wide as its content, and *Full width* stretches it across the text. |
| **Element tiles** | Shows the chemical element each session is named after, in the session's header and as a filter on the Sessions page, and the periodic table of your sessions in the command palette. It is off by default. |

## Work items

| Setting | What it does |
|---|---|
| **Close a parent with its last sub-item** | When the last open sub-item of a work item is done or cancelled, and at least one of them is done, the parent is done too, unless someone works on it or something of it waits for you. On by default. Turn it off to close parents yourself. See [Work items](/docs/using/work-items/#status). |
| **Fold done parents** | On the Work items page, a done or cancelled work item starts with its sub-items folded, and you can unfold it. On by default. |
| **Count sub-items at every level** | On the Work items page, a parent's progress counts every work item under it, at any depth, not only its own sub-items. The item's panel shows both counts either way. Off by default. |

## Terminal

These settings apply to the terminal of an agent that runs in Pinrail. See [The Summary and Terminal views](/docs/using/sessions/#the-summary-and-terminal-views).

![Settings, Terminal: the renderer, Option as Meta, the screen reader, clipboard writes, and the view a session opens in.](screenshot:settings-terminal)

| Setting | What it does |
|---|---|
| **Renderer** | Chooses what draws a terminal. **Automatic** uses libghostty, and xterm.js where libghostty cannot run. The row names the renderer in use and, when libghostty could not load, says why. A change applies to terminals opened afterwards. |
| **Option as Meta** | On macOS, chooses which Option keys act as Meta in a terminal: they send Esc and the key rather than the character Option types. Off by default. |
| **Screen reader** | Makes a terminal's text readable by screen readers such as VoiceOver and Orca, and announces new output. Off by default. |
| **Let programs copy to the clipboard** | Lets programs in a terminal copy text to the clipboard, up to 1 MiB at a time. They can never read it. On by default. |
| **Open a session in** | Chooses the view a session opens in: **Last used**, the view you last used for it, or else the summary; **Summary**; **Terminal**; or **Split view**, the summary above the terminal. |
| **Open an agent you start in** | Chooses the view the session of an agent you just started opens in: **Summary**, **Terminal**, the default, or **Split view**. |

## Shortcuts

| Setting | What it does |
|---|---|
| **Open Pinrail** | Under *Anywhere on your computer*: the keys that bring Pinrail forward from any app, <kbd>⌥⇧W</kbd> by default. Click the keys and press a new combination to change them. |
| **It opens** | The oldest pending review, or the inbox. |

The section also lists the app's keyboard shortcuts. See [The inbox](/docs/using/inbox/#keys).

## Agents

![Settings, Agents: the switch that updates agents automatically, three connected agents with their skill and hooks, and Codex ready to connect.](screenshot:settings-agents)

[Supported agents](/docs/agents/supported/) lists what Pinrail does for each agent, and what it installs for it.

| Setting | What it does |
|---|---|
| **Amp**, **Antigravity CLI**, **Augment CLI**, **Claude Code**, **Cline**, **Codex**, **Copilot CLI**, **Cursor**, **Goose**, **Grok CLI**, **Hermes Agent**, **Kiro CLI**, **OpenCode**, **Pi** | *Connect* adds a global skill named `pinrail` to the agent and, for the agents Pinrail can report on, Pinrail's hooks, extension or plugin. *Update* appears when an update of Pinrail changed either of them, or when one is missing, and brings both up to date. When several agents need it, *Update all* updates them together. *Remove* takes both away. |
| **Update agents automatically** | When Pinrail starts, it updates the skill and the hooks of each connected agent that an update of Pinrail changed. Turning it on also updates the agents that are outdated now. On by default. |
| **Arguments** | Under **More** on each connected agent that Pinrail starts: arguments added to every start and resume of that agent from Pinrail, such as `--model opus`, written as you would in a terminal. Flags that turn the agent's permission checks off, such as `--dangerously-skip-permissions`, are allowed here, and the agent's card says *Permission checks off*. Pinrail refuses the flags it sets itself to name or resume a session, such as Claude Code's `--session-id`. |

The skill tells an agent how to submit a review and read your decision, and how to build a plugin. It holds the same guides that `pinrail docs` prints. It does not tell the agent when to ask: a prompt, your instructions or another skill does that by telling the agent to use Pinrail.

Pinrail lists the agents it knows and finds each one by its configuration folder, such as `~/.claude` for Claude Code. The list shows the connected agents first, then the agents found but not connected, each in alphabetical order. The agents not found on this computer are in a folded list below them. It writes the skill to the agent's global skills folder, such as `~/.claude/skills/pinrail`, as a `SKILL.md` file and the guide to building a plugin in a `references` folder. A plugin that ships a skill of its own, such as the change map, has it in a `plugins` folder beside them, and `SKILL.md` names those plugins and tells the agent to read a plugin's skill before it submits to that plugin. The agent finds the other plugins with `pinrail plugins`. Installing, updating or removing a plugin rewrites the connected agents' skills. Pinrail marks the skill as its own, and it never changes or removes a `pinrail` skill that you wrote yourself: connecting such an agent adds only Pinrail's hooks. Some agents also read other agents' skills: OpenCode, Goose, Amp and Augment CLI read Claude Code's, and Grok CLI reads Claude Code's and Cursor's. Pinrail installs the skill in their own folders all the same, such as `~/.grok/skills/pinrail`, so they have it even without the other agents.

The hooks let Pinrail show when an agent works, waits for you or finishes, and show its turns, prompts and replies in its session. Pinrail adds them beside your own hooks in the agent's settings file, such as `~/.claude/settings.json`, and removes only its own. Pi takes an extension file and OpenCode a plugin instead. Codex runs a hook only once you have trusted it, so Pinrail trusts its hooks when it connects Codex. For Codex, connecting also writes a rule to `~/.codex/rules/pinrail.rules`, which runs the `pinrail` command outside Codex's sandbox, so that the command reaches the app. If Pinrail cannot read the file, it leaves the file as it is and connects the skill alone.

## Plugins

![Settings, Plugins: the installed plugins with where each came from.](screenshot:settings-plugins "The installed plugins, each with where it came from.")

Every installed plugin has a row with its icon, its title and whether it is ready. When a plugin is broken, hover over **broken** to see why. A plugin that ships a skill for your agents shows **skill**, and its tooltip says whether the skill is turned on. Every row has a **Notify** button, which turns notifications for that plugin's reviews on or off. The other buttons on a row depend on how the plugin was installed:

- An official plugin, and an installed copy, have **Remove**. An official plugin also shows **Update** when an app update brings a newer version. To upgrade any other plugin, install the new version.
- A linked folder has **Install a copy** and **Remove**.
- In the app, every row also has **Show in Finder**, or **Show in the file manager** on Linux.

See [Installing plugins](/docs/using/installing-plugins/) for what each of them does.

Click a row to open its details:

- **Version** and **Source**: the version installed, and the folder or zip it came from.
- **Files**: the files the plugin takes beside a review, for a plugin that takes any.
- **Clipboard**: shown for a plugin whose view may copy text and images from its reviews to the clipboard, because its manifest declares the `clipboard` capability. Pinrail shows a notice for every copy.
- **Skill**: shown for a plugin that ships a skill for your agents. Its switch adds the skill to your agents' `pinrail` skill or removes it. In the app, the folder button beside it opens the skill's folder so you can read it before you turn it on. See [Plugins with a skill](/docs/using/installing-plugins/#plugins-with-a-skill).
- **Opens without asking**: the sites whose links the plugin may open without asking you first. You allow a site from the question Pinrail shows when a plugin wants to open a link. Remove a site with its ✕. Removing the plugin removes all of them. Installing a new version from a folder or a zip keeps them, but installing a plugin in place of the copy that comes with Pinrail, or the other way round, does not.
- **Send a sample**: sends the review the plugin ships to show itself, and opens it. Plugins without a sample don't have the button.
- **Settings**: the plugin's own settings, when it has any. The code review plugin, for example, lets you choose between an inline and a side-by-side diff.

The *Install* field at the top installs a new plugin, and *Reload* reads every plugin from disk again.

## Data

| Setting | What it does |
|---|---|
| **Data directory** | Where your sessions, work items, reviews and settings are stored. *Show in Finder*, or *Show in the file manager* on Linux, opens it. |
| **Server** | The address of Pinrail's server. *Copy URL* copies it. |
| **Port** | The port Pinrail's server listens on, `4747` by default, from 1024 to 65535. The change takes effect after a restart, and the `pinrail` command follows it on its own. |
| **Keep reviews for** | *Forever*, 30 days, 90 days or a year. Ended reviews older than this are deleted from your history, with the files they carried. |
| **Files sent with reviews** | How many files agents have sent beside reviews, and the space they take. |
| **Install the CLI** | Puts the `pinrail` command into `~/.local/bin`, so agents can run it. |

## Privacy

| Setting | What it does |
|---|---|
| **Send usage data** | Sends anonymous usage data: which features are used and how often. On by default. |
| **Send crash reports** | Sends a report when Pinrail crashes, with file paths and quoted text removed. On by default. |
| **What is sent** | *Show what is sent* shows the events waiting to be sent, the last 100 sent and a crash report waiting, as JSON. |

When `DO_NOT_TRACK` or `PINRAIL_TELEMETRY` is set in the environment, both switches are off, and the page says which variable turned them off. See [Usage data and crash reports](/docs/using/privacy/).

## About

This section shows the version, the update status, where to start writing a plugin, and the license with its third-party notices.

| Setting | What it does |
|---|---|
| **What's new** | Shows the cards about the newest release again. Pinrail shows them by itself the first time a new release starts. They also open from *Help › What's New in Pinrail* and from <kbd>⌘K</kbd>. |
| **Updates** | Shows the update status. *Check for updates* checks now, and *Restart to update* installs a downloaded version. |
| **Check automatically** | Look for a new version when Pinrail starts and every few hours, and download it in the background. On by default. |

A downloaded version is installed when you restart Pinrail, or the next time you quit it. Pinrail never restarts by itself. When a version is ready, the sidebar and the menu bar menu say so, and you choose when to restart. An agent waiting on a review keeps waiting through the restart and gets its answer once Pinrail is back.

Checking downloads a small file from GitHub, where Pinrail's releases are published. Nothing about your reviews is sent. Every download is checked against a signature before it is installed. If you installed the `.deb` or `.rpm`, Pinrail tells you when a new version is available and links to it. Download it and install it in the same way as the first version.

## Where settings are stored

Settings are stored in `settings.json` in your data directory. You can read the file, back it up and edit it by hand, even while Pinrail is running. Pinrail applies an edit as soon as the file is saved. A value that the app would not accept, such as `0` for the days to keep history, is ignored, and the setting keeps its default. If the file is not valid JSON, Pinrail keeps its current settings, or uses the defaults when it is starting. Before it next saves a setting, it moves the invalid file to `settings.json.bad`, so your edit is not lost.

A script can read the settings through the local API:

```sh
curl http://127.0.0.1:4747/api/v1/settings
```

Changing a setting through the API is something only the app does, with a token that only its window holds. To change a setting from a script, edit `settings.json`. See [Who can act as you](/docs/concepts/trust/#who-can-act-as-you).

Some settings have no switch in the app and are set only in the file. One of them is `runtime.cli_control`, which lets the `pinrail` command start, stop and attach to agents. With it on, any program that you run, including an agent, can act as you.

Every setting, with its key, type and default, is in the [Settings reference](/docs/reference/settings/).
