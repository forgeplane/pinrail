---
title: Installing plugins
description: "Install a plugin from a folder or a zip, upgrade it, and know what installing does on your machine."
---

Pinrail installs one plugin at a time, from the app or from the command line. You give Pinrail a source, which is a plugin's folder or a zip of it on your computer, and it shows you what it found before it installs anything.

## From the app

Open *Settings › Plugins* and choose *Install…*. Paste the path of a folder or a zip, or choose a folder, then choose *Inspect*. Pinrail shows you:

- the plugin the manifest describes, and its version;
- where it comes from;
- what is already installed under the same name, and whether this version is older.

To serve a folder live while you work on it, turn on *Link instead of copying*. Choose *Install*, or *Link*, to confirm. Nothing is copied before that.

![The install dialog, previewing a plugin from a folder before it is installed: its name, version and source.](screenshot:install "The install dialog shows what the manifest declares, where the plugin comes from, and what installing it replaces.")

## From the command line

```sh
pinrail plugins install <folder or zip>
```

| Source | Installs |
|---|---|
| `./plugins/review` | A folder on this machine, copied into the app. |
| `./plugins/review --link` | The same folder, served directly while you work on it. |
| `~/Downloads/review-1.2.0.zip` | A zip of a plugin, such as one attached to a release. |

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

A plugin's full name is its publisher and its name, such as `forgeplane/list`. The publisher comes from where the plugin comes from, never from its manifest:

| Source | Publisher |
|---|---|
| The plugins that come with Pinrail | `forgeplane` |
| A folder, a zip, or a linked folder | `local` |

So a plugin you install can have the same name as one that comes with Pinrail. Agents and commands may name a plugin by its name alone, such as `pinrail submit review`, when only one installed plugin has that name. When two do, the name alone is refused with both full names, and the full name, such as `local/review`, picks one. Settings, link permissions and muted notifications belong to the full name.

## Versions and upgrades

Plugin versions are semantic, such as `"1.2.0"`. A review keeps the version it was submitted to, and always renders and validates with it. An upgrade is for new reviews, so it never changes a review already made, and a decided review shows what you saw when you decided.

```mermaid title="Each review keeps its version"
flowchart LR
  a["1.0.0"] --> b["1.1.0"] --> c["2.0.0"]
  r1(["reviews made with 1.0.0"]) -.-> a
  r2(["reviews made with 1.1.0"]) -.-> b
  r3(["new reviews"]) -.-> c
```

To upgrade a plugin, install the new version from its folder or its zip. The new version replaces the installed one for new reviews.

- **A newer version** replaces the installed one.
- **An older version** also replaces it. The install dialog says that the version is older before you install it, and the command line says so afterwards, for example *Replaced local/review 1.3.0 with the older 1.2.0*.
- **A version is kept** for as long as a review made with it is kept.

## Developing with a linked folder

A linked plugin is served straight from your folder, so a change shows the next time you open one of its reviews, with no reinstall:

```sh
pinrail plugins install ./ticket_triage --link
```

Reviews of a linked plugin render from the folder as it is now. Each review also keeps the folder as it was when the review was submitted, so once you remove the link, it renders with that. When you are done iterating, choose *Install a copy* on the plugin's row to keep the current state.

Pinrail checks a linked folder every second, so a change to the manifest, a schema or the decision template applies without a reload. A manifest that breaks shows its error on the plugin's row until you fix it.

### Working on a published plugin

To fix or change a plugin that someone published, such as `forgeplane/review`, link your copy of it in its place:

```sh
pinrail plugins install ./review --link --replace forgeplane/review
```

The link takes the plugin's full name, so its existing reviews and new ones render with your folder. The plugin's row says that a local folder replaces it. Removing the link puts the installed version back:

```sh
pinrail plugins remove review
```

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
