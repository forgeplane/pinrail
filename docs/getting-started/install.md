---
title: Install
description: "Install the Pinrail app and the pinrail command, and check that they can talk to each other."
---

Pinrail has two parts. The **app** is where reviews wait for you and where you decide them. The **`pinrail` command** is what agents and scripts use to ask. The app carries the command, so installing the app is usually all you need.

:::note[Early development]
Pinrail is in early development. Expect rough edges, and expect things to change between releases.
:::

## The app

Download the latest release from [GitHub](https://github.com/forgeplane/pinrail/releases).

| System | Download |
|---|---|
| macOS 13 or later | The `.dmg`. Open it and drag Pinrail to Applications. |
| Linux | The AppImage, or the `.deb` or `.rpm` for your distribution. |

Open Pinrail. It starts a small server on your machine, at `127.0.0.1:4747`, which is how the `pinrail` command reaches it. Closing the window keeps the app running in the menu bar, so agents can still ask while the window is closed.

## The command

In the app, open **Settings › Data** and choose **Install the CLI**. It puts `pinrail` into `~/.local/bin`. Make sure that folder is on your `PATH`:

```sh
pinrail --version
```

:::tip[Building from source]
Before the first release, or to follow the latest changes, build the command from a checkout with a Rust toolchain:

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
