---
title: Install
description: "Install the Pinrail app, run its setup, and check that the pinrail command reaches the app."
---

Pinrail has three parts. Installing the app gives you all of them.

| Part | What it is |
|---|---|
| **The app** | The window, and the core behind it, which keeps your sessions, work items and reviews and serves them on `127.0.0.1:4747`. |
| **The terminal host** | A separate program, Pinrail Host, that runs the agents you start in Pinrail and keeps them running when the app quits. The app starts it when the app starts. See [The terminal host](/docs/concepts/terminal-host/). |
| **The `pinrail` command** | What agents and scripts run to reach the app. The app carries it, and installs it in its setup. |

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

Open Pinrail. It starts a small server on your machine, at `127.0.0.1:4747`, which is how the `pinrail` command reaches it. It also starts the terminal host. The host runs from its own copy of the program, in the `host/bin` folder of Pinrail's data directory, and the app starts it through `launchd` on macOS and as a unit of your `systemd` user manager on Linux, so that it does not end with the app. Closing the window keeps the app running in the menu bar, or in the system tray on Linux, so agents can still ask while the window is closed.

The macOS app and the AppImage update themselves. They download new versions in the background and install them when you restart or quit Pinrail. With the `.deb` or `.rpm`, Pinrail tells you when a new version is available, and you install it in the same way as the first one. On macOS, the `pinrail` command is a link into the app, so it updates with the app. With the AppImage, Pinrail updates the copy of the command it installed the next time it starts. See [Settings › About](/docs/using/settings/#about) to check by hand or turn automatic checks off.

An update ends no agent. The updated app starts the terminal host of the new version, which takes over the terminals of the agents that run, and the agents that you start afterwards run under the new host.

## The setup

The first time you open Pinrail, it shows a welcome screen that sets Pinrail up with one button.

![The welcome screen. It shows the four agents found on this computer, each with a tick, beside Pinrail and you. Under them are the Set up Pinrail button and the Add more plugins button.](screenshot:setup-welcome "The welcome screen, with the agents found on this computer.")

1. **Check the agents.** The screen shows the agents that Pinrail found on your computer. Click one to leave it out. You can connect an agent later in [Settings › Agents](/docs/using/settings/#agents).
2. **Choose the plugins, if you like.** Pinrail installs the two plugins it recommends, List and Feedback. **Add more plugins** lists the others. Each plugin is one kind of review that an agent can ask you for.
3. **Click Set up Pinrail.** Pinrail then does the rest by itself, and shows each step with its result:
   - it installs the `pinrail` command;
   - it connects each agent you kept, which adds Pinrail's skill and its hooks to the agent;
   - it installs the plugins;
   - it turns notifications on, and your system asks you to allow them.
4. **Start your first session.** When every step is done, **Continue** opens the last screen. See [Your first review](/docs/getting-started/first-review/).

When a step fails, the screen says why. **Try again** runs the failed steps again, and **Continue** goes on without them, so that you can finish them later in Settings. **Skip setup** on the welcome screen closes the setup.

To open the setup again, press <kbd>⌘K</kbd> and choose *Set up Pinrail*.

## The command

Agents and scripts reach Pinrail with the `pinrail` command, which the setup installs.

| You installed | Where the command is |
|---|---|
| The macOS app | `~/.local/bin/pinrail`, a link to the command inside the app. It updates with the app. |
| The AppImage | `~/.local/bin/pinrail`, a copy that Pinrail updates the next time it starts. |
| The `.deb` or `.rpm` | `/usr/bin/pinrail`. The package installs it. |

### When a terminal cannot find the command

A terminal finds `pinrail` only when `~/.local/bin` is on your `PATH`. When it is not, the setup's step for the command reads *Not on your PATH*, and shows the line that adds the folder, written for your shell.

![The setting-up screen with one step left. The step for the pinrail command says Not on your PATH, with the line to add, and the buttons Copy the line, Add it to ~/.zshrc and Check again. The four agents below it are connected.](screenshot:setup-path "The command is installed, and a new terminal cannot find it yet.")

- **Add it to ~/.zshrc** adds the line to your shell's profile file for you. The button names the file of your shell: `~/.zshrc` for zsh, `~/.bash_profile` or `~/.bashrc` for bash, and `config.fish` for fish. Open a new terminal afterwards. The terminals that are already open do not find the command yet.
- **Copy the line** copies it, so that you can add it yourself. Use this when your shell is another one, and the button to add the line does not show, or when Pinrail could not change the file. In that case the screen says why under the line.
- **Check again** looks once more, after you changed the `PATH` yourself.

When the step reads *Another one runs first*, a different `pinrail` is earlier on your `PATH`, for example one that you built from source. Put `~/.local/bin` earlier on the `PATH`, or remove the other one.

You can also install the command later from **Settings › Data** with **Install the CLI**.

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

## On macOS: add Pinrail Host to Developer Tools

Do this if your agents build and run software, such as a test suite. Without it, macOS checks every newly built program the first time it runs, which makes an agent's builds and tests much slower in Pinrail than in a terminal that you already allowed. [The terminal host](/docs/concepts/terminal-host/#pinrail-host-in-macos-privacy-prompts) explains why.

1. Open *System Settings › Privacy & Security › Developer Tools*.
2. Click **+**, and unlock with your password or Touch ID if macOS asks.
3. In the file dialog, press <kbd>⌘⇧G</kbd>, enter this path and choose the app:

   ```txt
   /Applications/Pinrail.app/Contents/Helpers/Pinrail Host.app
   ```

4. Check that **Pinrail Host** is in the list and switched on.
5. Stop the agents that run in Pinrail and resume them, so that they start again under the new permission.

On some versions of macOS, *Developer Tools* is listed under *Privacy & Security* only after a program has asked for it. If you do not see it, run `spctl developer-mode enable-terminal` in Terminal, which makes macOS show the list, and open System Settings again.

The first time an agent reads your Documents, Desktop or Downloads folder, macOS also asks whether Pinrail Host may use it. Allow it for the folders your agents work in.

## Check that it works

With the app running, list your reviews:

```sh
pinrail list
```

An empty list means that the command found the app. If the command cannot reach the app, see [Finding the app](/docs/agents/cli/#finding-the-app).

## Uninstalling

1. In *Settings › Agents*, click **Remove** on each connected agent. This takes Pinrail's skill and hooks out of the agent's own folders.
2. Quit Pinrail with **Stop agents and quit**. With no agent left and the app closed, the terminal host stops by itself within ten minutes. It leaves no `launchd` job and no `systemd` unit behind.
3. Remove the app: drag it out of Applications on macOS, delete the AppImage, remove the `.deb` or `.rpm` package with your package manager, or on Windows uninstall Pinrail in *Settings › Apps*.
4. Delete the data directory, `~/.local/share/pinrail` by default, or `%LOCALAPPDATA%\dev.pinrail.desktop` on Windows, where the uninstaller's *Delete the application data* box does it for you. It holds your sessions, work items, reviews and settings, and the copies of the terminal host.
5. Delete `~/.local/bin/pinrail`, unless the package installed the command, and the line that adds `~/.local/bin` to your `PATH` if the setup added it to your shell's profile.

## Next

- [Your first review](/docs/getting-started/first-review/): have your agent ask you something, and decide it.
- [Instructing an agent](/docs/agents/instructing/): tell your agent when to ask.
