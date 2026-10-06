---
title: Install
description: "Install the Pinrail app and the pinrail command, and check that they can talk to each other."
---

Pinrail has two parts. The **app** is where reviews wait for you and where you decide them. The **`pinrail` command** is what agents and scripts use to ask. The app carries the command, so installing the app is usually all you need.

:::note[Early development]
Pinrail is in early development. Expect rough edges, and expect things to change between releases.
:::

## The app

Download it from [the download page](/download/), which has every system and format.

| System | Download |
|---|---|
| macOS 13 or later | [pinrail-app-universal.dmg](https://github.com/forgeplane/pinrail/releases/latest/download/pinrail-app-universal.dmg). Open it and drag Pinrail to Applications. |
| Linux, x86-64 | [AppImage](https://github.com/forgeplane/pinrail/releases/latest/download/pinrail-app-amd64.AppImage) for any distribution (`chmod +x` it, then run it), [.deb](https://github.com/forgeplane/pinrail/releases/latest/download/pinrail-app-amd64.deb) (`sudo apt install ./pinrail-app-amd64.deb`) or [.rpm](https://github.com/forgeplane/pinrail/releases/latest/download/pinrail-app-x86_64.rpm) (`sudo dnf install ./pinrail-app-x86_64.rpm`). |
| Linux, ARM64 | [AppImage](https://github.com/forgeplane/pinrail/releases/latest/download/pinrail-app-aarch64.AppImage), [.deb](https://github.com/forgeplane/pinrail/releases/latest/download/pinrail-app-arm64.deb) (`sudo apt install ./pinrail-app-arm64.deb`) or [.rpm](https://github.com/forgeplane/pinrail/releases/latest/download/pinrail-app-aarch64.rpm) (`sudo dnf install ./pinrail-app-aarch64.rpm`). |
| Windows | Coming soon. |

Open Pinrail. It starts a small server on your machine, at `127.0.0.1:4747`, which is how the `pinrail` command reaches it. Closing the window keeps the app running in the menu bar, or in the system tray on Linux, so agents can still ask while the window is closed.

The macOS app and the AppImage update themselves. They download new versions in the background and install them when you restart or quit Pinrail. With the `.deb` or `.rpm`, Pinrail tells you when a new version is available, and you install it in the same way as the first one. On macOS, the `pinrail` command is a link into the app, so it updates with the app. With the AppImage, Pinrail updates the copy of the command it installed the next time it starts. See [Settings › About](/docs/using/settings/#about) to check by hand or turn automatic checks off.

## The command

The first time you open Pinrail, it shows a short setup in three steps:

1. *Connect* installs the `pinrail` command, adds Pinrail's skill to the agents you choose, and turns notifications on. It lists only the agents found on your computer. [Settings › Agents](/docs/using/settings/#agents) connects them later too.
2. *Plugins* installs the plugins you choose. The two that Pinrail recommends are already selected.
3. *Try it* gives you, for each installed plugin, a prompt to give your agent and a sample review to send yourself. This step opens once a plugin is installed.

![The setup's first step: the pinrail command, the agents found on this computer, and notifications.](screenshot:setup-connect)

To open the setup again, press <kbd>⌘K</kbd> and choose *Set up Pinrail*.

You can also install the command from **Settings › Data** with **Install the CLI**. It puts `pinrail` into `~/.local/bin`, so make sure that folder is on your `PATH`. With the `.deb` or `.rpm`, the command is already installed as `/usr/bin/pinrail`.

Check that the command works:

```sh
pinrail --version
```

:::tip[Building from source]
To follow the latest changes, build the command from a checkout with a Rust toolchain:

```sh
cargo install --path cli
```
:::

## Check that it works

With the app running, list your reviews:

```sh
pinrail list
```

An empty list means that the command found the app. If the command cannot reach the app, see [Finding the app](/docs/agents/cli/#finding-the-app).

## Next

- [Your first review](/docs/getting-started/first-review/): send a review yourself and decide it.
- [Instructing an agent](/docs/agents/instructing/): tell your agent when to ask.
