---
title: Settings
description: "Every section of Pinrail's settings, what each option does, and where the settings are stored."
---

Open settings with <kbd>⌘,</kbd>, with the gear at the bottom of the sidebar, or from *Navigate › Settings…* in the menu. Changes take effect immediately. There is no Save button.

## General

![Settings, General: the inbox, startup and notifications.](screenshot:settings-general)

### Inbox

| Setting | What it does |
|---|---|
| **Opening a review** | *Beside the list* opens a review from the inbox or the history, or a session from the Sessions page, in a panel next to the list, which stays in view. *As a page* opens it on a page of its own. |

### Startup

| Setting | What it does |
|---|---|
| **Launch at login** | Start Pinrail when you log in, so agents can always ask. |
| **Closing the window** | *Hide to the menu bar* keeps Pinrail running, so reviews still arrive. *Quit* stops it. On Linux, the option is *Hide to the tray*. |
| **Show in the menu bar** | The menu bar icon, with the number of waiting reviews and a menu to open them. On Linux, the setting is *Show in the tray*. |

### Notifications

| Setting | What it does |
|---|---|
| **System notifications** | Announce new reviews. |
| **Pause** | Stop notifications for 15 minutes, an hour, or until tomorrow. Reviews still arrive and are counted. |
| **Sound** | Play the system sound with each notification. |

See [Notifications](/docs/using/notifications/) for more, including muting one plugin.

## Appearance

| Setting | What it does |
|---|---|
| **Colour scheme** | *System*, *Dark* or *Light*. Plugin views follow it. <kbd>⌘⇧L</kbd> switches it from anywhere in the app. |
| **Dark theme**, **Light theme** | The theme for each colour scheme. See [Themes](/docs/using/themes/). |
| **Custom themes** | *Open folder* opens the folder of your own themes, and *New theme…* creates one from the current theme. Any error in a theme file shows here. |
| **Text size** | *Small*, *Default* or *Large*. |

## Shortcuts

| Setting | What it does |
|---|---|
| **Open Pinrail** | Under *Anywhere on your computer*: the keys that bring Pinrail forward from any app, <kbd>⌥⇧W</kbd> by default. Click the keys and press a new combination to change them. |
| **It opens** | The oldest pending review, or the inbox. |

The section also lists the app's keyboard shortcuts. See [The inbox](/docs/using/inbox/#keys).

## Agents

![Settings, Agents: the agents Pinrail found on this computer, with the pinrail skill connected to some of them.](screenshot:settings-agents)

| Setting | What it does |
|---|---|
| **Amp**, **Antigravity CLI**, **Augment CLI**, **Claude Code**, **Cline**, **Codex**, **Copilot CLI**, **Cursor**, **Goose**, **Grok CLI**, **Hermes Agent**, **Kiro CLI**, **OpenCode**, **Pi** | *Connect* adds a global skill named `pinrail` to the agent. *Update* appears when an update of Pinrail changed the skill, and rewrites it. When several agents need it, *Update all* updates them together. *Remove* takes the skill away. |
| **Update skills automatically** | When Pinrail starts, it updates the skill of each connected agent that an update of Pinrail changed. Turning it on also updates the skills that are outdated now. Off by default. |

The skill tells an agent how to submit a review and read your decision, and how to build a plugin. It holds the same guides that `pinrail docs` prints. It does not tell the agent when to ask: a prompt, your instructions or another skill does that by telling the agent to use Pinrail.

Pinrail lists the agents it knows and finds each one by its configuration folder, such as `~/.claude` for Claude Code. The list shows the connected agents first, then the agents found but not connected, each in alphabetical order. The agents not found on this computer are in a folded list below them. It writes the skill to the agent's global skills folder, such as `~/.claude/skills/pinrail`, as a `SKILL.md` file and the guide to building a plugin in a `references` folder. A plugin that ships a skill of its own, such as the change map, has it in a `plugins` folder beside them, and `SKILL.md` names those plugins and tells the agent to read a plugin's skill before it submits to that plugin. The agent finds the other plugins with `pinrail plugins`. Installing, updating or removing a plugin rewrites the connected agents' skills. Pinrail marks the skill as its own, and it never changes or removes a `pinrail` skill that you wrote yourself. Some agents also read other agents' skills: OpenCode, Goose, Amp and Augment CLI read Claude Code's, and Grok CLI reads Claude Code's and Cursor's. Pinrail installs the skill in their own folders all the same, such as `~/.grok/skills/pinrail`, so they have it even without the other agents.

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
| **Data directory** | Where reviews, decisions and settings are stored. *Show in Finder*, or *Show in the file manager* on Linux, opens it. |
| **Server** | The address of Pinrail's server. *Copy URL* copies it. |
| **Port** | The port Pinrail's server listens on, `4747` by default, from 1024 to 65535. The change takes effect after a restart, and the `pinrail` command follows it on its own. |
| **Keep reviews for** | *Forever*, 30 days, 90 days or a year. Ended reviews older than this are deleted from your history, with the files they carried. |
| **Files sent with reviews** | How many files agents have sent beside reviews, and the space they take. |
| **Install the CLI** | Puts the `pinrail` command into `~/.local/bin`, so agents can run it. |

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

Settings are stored in `settings.json` in your data directory. You can read the file, back it up and edit it by hand, even while Pinrail is running. Pinrail applies an edit as soon as the file is saved. A value that the app would not accept, such as `0` for the days to keep history, is ignored, and the setting keeps its default. If the file is not valid JSON, Pinrail keeps its current settings, or uses the defaults when it is starting. Before it next saves a setting, it moves the invalid file to `settings.json.bad`, so your edit is not lost. A script can read and change settings through the local API, which checks every change the same way the app does:

```sh
curl http://127.0.0.1:4747/api/v1/settings
curl -X PATCH http://127.0.0.1:4747/api/v1/settings \
  -H 'content-type: application/json' -d '{"appearance": {"theme": "dark"}}'
```

Every setting, with its key, type and default, is in the [Settings reference](/docs/reference/settings/).
