---
title: Settings
description: "Every section of Pinrail's settings, what each option does, and where the settings are stored."
---

Open settings with <kbd>⌘,</kbd>, the gear at the bottom of the sidebar, or *Pinrail › Settings…*. Every change applies at once; there is nothing to save.

## General

![Settings, General: startup and notifications.](screenshot:settings-general)

### Startup

| Setting | What it does |
|---|---|
| **Launch at login** | Start Pinrail when you log in, so agents can always ask. |
| **Closing the window** | *Hide to the menu bar* keeps Pinrail running, so reviews still arrive. *Quit* stops it. |
| **Show in the menu bar** | The menu bar icon, with the number of waiting reviews and a menu to open them. |

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
| **Theme** | *System*, *Dark* or *Light*. Plugin views follow it. `T` switches it from anywhere in the app. |
| **Text size** | *Small*, *Default* or *Large*. |

## Shortcuts

| Setting | What it does |
|---|---|
| **Global shortcut** | Brings Pinrail forward from any app. <kbd>⌥⇧W</kbd> by default. Click it and press new keys to change it. |
| **It opens** | The oldest pending review, or the inbox. |

Below, the section lists the app's own keys. See [The inbox](/docs/using/inbox/#keys).

## Plugins

![Settings, Plugins: the installed plugins with where each came from.](screenshot:settings-plugins "Five plugins: two built in, three installed from GitHub.")

Every installed plugin has a row: its icon and title, its version, where it came from, and whether it is ready. On each row:

- **Send a sample** sends the review the plugin ships to show itself, and opens it. Plugins without a sample don't have the button.
- **Notify** turns notifications for that plugin's reviews on or off.
- **Update**, **Remove** and **Install a copy** manage it. See [Installing plugins](/docs/using/installing-plugins/).
- **The plugin's own settings**, when it has any, are folded under its row. The code review plugin, for example, lets you choose between an inline and a side-by-side diff.

*Install…* at the top installs a new plugin, and *Reload* reads every plugin from disk again.

## Data

| Setting | What it does |
|---|---|
| **Data directory** | Where reviews, decisions and settings are stored. *Show in Finder* opens it. |
| **Port** | The port Pinrail's server listens on, `4747` by default. Takes effect after a restart; the `pinrail` command follows it on its own. |
| **Keep reviews for** | *Forever*, or a number of days. Ended reviews older than this are deleted from your history, with the files they carried. |
| **Files sent with reviews** | How many files agents have sent beside reviews, and the space they take. |
| **Install the CLI** | Puts the `pinrail` command into `~/.local/bin`. |

## About

The version, updates, where to start writing a plugin, and the licence with its third-party notices.

| Setting | What it does |
|---|---|
| **Updates** | Where updating stands. *Check for updates* looks now; *Restart to update* installs a version that is ready. |
| **Check automatically** | Look for a new version when Pinrail starts and every few hours, and download it in the background. On by default. |

A downloaded version is installed when you restart Pinrail, or the next time you quit it. Pinrail never restarts on its own: the sidebar and the menu bar menu say when a version is ready, and you choose when. An agent waiting on a review keeps waiting through the restart and gets its answer once Pinrail is back.

Checking downloads a small file from GitHub, where Pinrail's releases are published; nothing about your reviews is sent. Every download is checked against a signature before it is installed. If you installed the `.deb` or `.rpm`, your package manager installs new versions: Pinrail says when one is out and links to it.

## Where settings are stored

Settings live in `settings.json` in your data directory. You can read it, back it up, and edit it by hand while Pinrail is closed. Pinrail checks the file when it reads it: a value it would not accept in the app, such as `0` for the days to keep history, is ignored and the setting keeps its default. A script can read and change settings through the local API, which checks every change the same way the app does:

```sh
curl http://127.0.0.1:4747/api/v1/settings
curl -X PATCH http://127.0.0.1:4747/api/v1/settings \
  -H 'content-type: application/json' -d '{"appearance": {"theme": "dark"}}'
```

Every setting, with its key, type and default, is in the [Settings reference](/docs/reference/settings/).
