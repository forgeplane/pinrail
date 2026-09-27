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
| Windows | Coming soon. |

Open Pinrail. It starts a small server on your machine, at `127.0.0.1:4747`, which is how the `pinrail` command reaches it. Closing the window keeps the app running in the menu bar, or in the system tray on Linux, so agents can still ask while the window is closed.

The macOS app and the AppImage update themselves: they download new versions in the background and install them when you restart Pinrail. With the `.deb` or `.rpm`, Pinrail tells you when a new version is out, and you install it as you did the first one. On macOS the `pinrail` command links into the app, so it updates too; from an AppImage, choose **Install the CLI** again after an update. See [Settings › About](/docs/using/settings/#about) to check by hand or turn automatic checks off.

## The command

The first time you open Pinrail, it shows a short setup that installs the `pinrail` command, asks for permission to send notifications, and sends you a first review. To open the setup again, press <kbd>⌘K</kbd> and choose *Set up Pinrail*.

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

An empty list means the command found the app. If it can't reach it, see [Finding the app](/docs/agents/cli/#finding-the-app).

## Next

- [Your first review](/docs/getting-started/first-review/): send a review yourself and decide it.
- [Instructing an agent](/docs/agents/instructing/): tell your agent when to ask.
