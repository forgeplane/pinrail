---
title: Settings
description: "Every section of Pinrail's settings, what each option does, and where the settings are stored."
---

Open settings with <kbd>⌘,</kbd>, with the gear at the bottom of the sidebar, or from *Navigate › Settings…* in the menu. Changes take effect immediately. There is no Save button.

## General

![Settings, General: startup and notifications.](screenshot:settings-general)

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
| **Theme** | *System*, *Dark* or *Light*. Plugin views follow it. <kbd>⌘⇧L</kbd> switches it from anywhere in the app. |
| **Text size** | *Small*, *Default* or *Large*. |

## Shortcuts

| Setting | What it does |
|---|---|
| **Open Pinrail** | Under *Anywhere on your computer*: the keys that bring Pinrail forward from any app, <kbd>⌥⇧W</kbd> by default. Click the keys and press a new combination to change them. |
| **It opens** | The oldest pending review, or the inbox. |

The section also lists the app's keyboard shortcuts. See [The inbox](/docs/using/inbox/#keys).

## Plugins

![Settings, Plugins: the installed plugins with where each came from.](screenshot:settings-plugins "Five plugins: two built in, three installed from GitHub.")

Every installed plugin has a row with its icon, its title and whether it is ready. When a plugin is broken, hover over **broken** to see why. Every row has a **Notify** button, which turns notifications for that plugin's reviews on or off. The other buttons on a row depend on how the plugin was installed:

- An installed copy has **Remove**. To upgrade it, install the new version.
- A linked folder has **Install a copy** and **Remove**.
- A built-in plugin has neither.
- In the app, every row also has **Show in Finder**, or **Show in the file manager** on Linux.

See [Installing plugins](/docs/using/installing-plugins/) for what each of them does.

Click a row to open its details:

- **Version** and **Source**: the version installed, and the folder or zip it came from.
- **Files**: the files the plugin takes beside a review, for a plugin that takes any.
- **Opens without asking**: the sites whose links the plugin may open without asking you first. You allow a site from the question Pinrail shows when a plugin wants to open a link. Remove a site with its ✕. Removing the plugin removes all of them. Installing a new version from a folder or a zip keeps them, but installing a plugin in place of the copy that comes with Pinrail, or the other way round, does not.
- **Send a sample**: sends the review the plugin ships to show itself, and opens it. Plugins without a sample don't have the button.
- **Settings**: the plugin's own settings, when it has any. The code review plugin, for example, lets you choose between an inline and a side-by-side diff.

*Install…* at the top installs a new plugin, and *Reload* reads every plugin from disk again.

## Data

| Setting | What it does |
|---|---|
| **Data directory** | Where reviews, decisions and settings are stored. *Show in Finder*, or *Show in the file manager* on Linux, opens it. |
| **Server** | The address of Pinrail's server. *Copy URL* copies it. |
| **Port** | The port Pinrail's server listens on, `4747` by default, from 1024 to 65535. The change takes effect after a restart, and the `pinrail` command follows it on its own. |
| **Keep reviews for** | *Forever*, 30 days, 90 days or a year. Ended reviews older than this are deleted from your history, with the files they carried. |
| **Files sent with reviews** | How many files agents have sent beside reviews, and the space they take. |
| **Install the CLI** | Puts the `pinrail` command into `~/.local/bin`. |

## About

This section shows the version, the update status, where to start writing a plugin, and the license with its third-party notices.

| Setting | What it does |
|---|---|
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
