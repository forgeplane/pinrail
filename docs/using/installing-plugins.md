---
title: Installing plugins
description: "Install a plugin from a folder or a zip, upgrade it, and know what installing does on your machine."
---

Pinrail installs one plugin at a time, from the app or from the command line. You give Pinrail a source, which is a plugin's folder or a zip of it on your computer, and it shows you what it found before it installs anything.

## From the app

Open *Settings › Plugins*. In the *Install* field, paste the path of a folder or a zip, or choose one with *Folder…* or *Zip…*. Pinrail inspects the source as soon as you choose it or stop typing, and shows you:

- the plugin the manifest describes, and its version;
- where it comes from;
- what is already installed under the same name, and whether this version is older.

Choose *Install* to confirm. When a plugin of the same name is already installed, the button says *Replace* or *Install again*. Nothing is copied before you confirm. When the plugin is installed, Pinrail says so and highlights it in the list.

To serve a folder live while you work on it, open the menu beside *Install* and choose *Link to the folder*.

![The install panel, previewing a plugin from a folder before it is installed: its name, version and source.](screenshot:install "The install panel shows what the manifest declares, where the plugin comes from, and what installing it replaces.")

## From the command line

```sh
pinrail plugins install <folder or zip>
```

| Source | Installs |
|---|---|
| `~/code/triage` | A folder on this machine, copied into the app. |
| `~/code/triage --link` | The same folder, served directly while you work on it. |
| `~/Downloads/triage-1.2.0.zip` | A zip of a plugin, such as one attached to a release. |

To install a plugin that is published on a website, download its zip first, then install the zip.

## What an install does

```mermaid title="From a source to the store"
flowchart TB
  S["source: folder or zip"] --> U["unpack a zip"]
  U --> I["inspect the manifest"]
  I --> C["check the bundle"]
  C --> K["store the bundle"]
  K --> V["new reviews render with it"]
```

1. **Unpack.** A zip is unpacked into a scratch folder. Its manifest can be at the top of the zip, or inside the one folder at the top.
2. **Check.** The manifest, the schemas and `view/index.html` must be valid. A folder without `view/index.html` is usually a plugin whose view is built by a tool such as Vite, and it is refused with a message that says to build it first.
3. **Store.** Only the bundle is kept: `manifest.json`, `icon.svg`, `README.md`, `LICENSE` and the folders `schemas/`, `view/`, `templates/` and `samples/`, without hidden files. Everything else, such as sources, tests, fixtures and `node_modules/`, stays behind. The bundle is stored once, named by the hash of its files, and becomes the version new reviews use. Pinrail records where it came from.

Installing runs nothing on your computer and connects to nothing.

## Plugin names

A plugin is known by the `name` in its manifest, such as `review`. Agents, commands, settings and reviews all use that name. Where a plugin came from, a folder, a zip or the app itself, is shown on its row but is not part of its name.

One plugin is installed under each name. Installing a plugin under a name that is already installed replaces what is there, and the install panel says what it replaces before you confirm. This also applies to the plugins that come with Pinrail: a plugin you install under the name `list` takes the place of Pinrail's own.

The plugin's settings belong to its name, so they stay when you install a new version. The sites it may open without asking also stay when you install it again from a folder or a zip. They do not carry over between the copy that comes with Pinrail and a plugin from disk under the same name, because that name then means another plugin.

## Versions and upgrades

Plugin versions are semantic, such as `"1.2.0"`. A review records the version of its plugin that it renders and validates with.

- **A pending review** moves to the version installed now when you open it, as long as that version accepts the review's payload and files. When it does not, the review stays on its version, and the review screen says why. If the plugin changes while a review is open, the review screen offers *Reload*.
- **An ended review**, one that was decided, discarded, withdrawn or expired, keeps its version. A decided review therefore always shows what you saw when you decided.

```mermaid title="Pending reviews follow the plugin, ended reviews keep their version"
flowchart LR
  a["1.0.0"] --> b["1.1.0"] --> c["2.0.0"]
  r1(["reviews decided with 1.0.0"]) -.-> a
  r2(["reviews decided with 1.1.0"]) -.-> b
  r3(["pending reviews, once opened"]) -.-> c
```

To upgrade a plugin, install the new version from its folder or its zip. The new version replaces the installed one for new reviews, and for the pending reviews it accepts.

- **A newer version** replaces the installed one.
- **An older version** also replaces it. The install panel says that the version is older before you install it, and the command line says so afterwards, for example *Replaced review 1.3.0 with the older 1.2.0*.
- **A version is kept** for as long as a review made with it is kept.

## Developing with a linked folder

A linked plugin is served straight from your folder, so a change shows the next time you open one of its reviews, with no reinstall:

```sh
pinrail plugins install ./ticket_triage --link
```

A linked plugin follows your folder. When you open a pending review of it after a change, Pinrail stores the folder as it is and the review moves to it. When you change the folder while the review is open, the review screen offers *Reload*. Once the review ends, or once you remove the link, it keeps the version of the folder it last showed. When you are done iterating, choose *Install a copy* on the plugin's row to keep the current state.

Pinrail checks a linked folder every second. A check only notes that the folder changed, which is how an open review knows to offer *Reload*. The folder is stored again once, the next time the plugin is used: when an agent describes it or submits a review to it, or when you open or reload one of its reviews. However often you save in between, only that one copy is stored. A change to the manifest, a schema or the decision template therefore applies without a reload, and a submission is always checked against the folder as it is. A folder with a mistake, such as a manifest that does not parse, is refused when it is used, with the reason, and the reason shows on the plugin's row until you fix it. Until then, the plugin keeps the last version of the folder that worked.

A linked folder must not contain symbolic links, as for any install.

### Working on a plugin that comes with Pinrail

To change one of the plugins that come with Pinrail, link your copy of it under the same name:

```sh
pinrail plugins install ./list --link
```

The link takes the plugin's place, so its reviews render with your folder. Removing the link removes the installation, and Pinrail installs its own copy again the next time it starts.

## Removing a plugin

```sh
pinrail plugins remove ticket_triage
```

Removing a plugin stops new reviews from using it. Existing reviews keep the versions they were made with, so your history stays readable. The plugins that come with Pinrail cannot be removed.

## What runs on your machine

Installing a plugin runs nothing on your computer: Pinrail copies the plugin's files into its store and checks them. The plugin's view runs later, in a sandboxed frame, when you open one of its reviews. [What runs where](/docs/concepts/trust/#what-installing-a-plugin-runs) explains what a plugin can and cannot do.

## Where plugins live

Under the app's data directory, `plugins/` holds two folders:

| Folder | Contents |
|---|---|
| `bundles/<hash>/` | The installed versions, each stored once and never changed. |
| `work/` | Scratch space where a zip is unpacked during an install. |
