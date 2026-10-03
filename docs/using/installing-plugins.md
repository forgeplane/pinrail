---
title: Installing plugins
description: "Install a plugin from a folder, a repository or a GitHub release, keep it up to date, and know what runs on your machine."
---

Pinrail installs one plugin at a time, from the app or from the command line. You give Pinrail a source (a folder, a Git repository or a GitHub release), and it shows you what it found before it installs anything.

## From the app

Open *Settings › Plugins* and choose *Install…*. Paste a source, or choose a folder, then choose *Inspect*. Pinrail fetches the source and shows you:

- the plugin the manifest describes, and its version;
- where it comes from;
- whether a build runs, and the exact command;
- what is already installed under the same name.

To serve a folder live while you work on it, turn on *Link instead of copying*. Choose *Install*, or *Link*, to confirm. Nothing is copied or run before that.

![The install dialog, previewing a plugin from a folder before it is installed: its name, version, source, and that no build runs.](screenshot:install "The install dialog shows what the manifest declares, where the plugin comes from, and what installing it would run.")

## From the command line

```sh
pinrail plugins install <source>
```

The form of the source tells Pinrail where to fetch the plugin from:

| Source | Installs |
|---|---|
| `./plugins/review` | A folder on this machine, copied into the app. |
| `./plugins/review --link` | The same folder, served directly while you work on it. |
| `github.com/acme/plugins/review@v3` | A folder inside a repository, at a tag. |
| `https://github.com/acme/plugins/tree/v3/review` | The same, as your browser shows it. |
| `github.com/acme/pinrail-review` | A repository's root, on its default branch. |
| `git@acme.internal:plugins.git --ref v3 --path review` | Any git remote over SSH, with the ref and folder given separately. |
| `https://github.com/acme/pinrail-review/releases` | The latest GitHub release: its prebuilt bundle, with no build. |
| `https://github.com/acme/pinrail-review/releases/tag/v1.2.0` | That release, pinned. |

A ref is a branch, a tag or a commit. Without one, Pinrail uses the default branch. GitLab's `/-/tree/<ref>/<folder>` URLs work the same way as GitHub's.

When the plugin declares a build, the command prints the exact build command and asks you to confirm it before anything runs. The `--yes` option confirms the build in advance, for a script or an agent's session where nobody can answer the question. Without `--yes`, and with nobody at a terminal to answer, the install is refused and nothing runs.

Pinrail installs exactly what it showed you: the same commit of a repository, the same release asset, and the same build command. If the source changes between the check and the install, the install is refused before anything runs, and you install again to see the new version.

:::tip[Prefer releases]
A release carries a prebuilt bundle, so installing it needs no toolchain and runs nothing on your machine. See [Publishing a plugin](/docs/building/publishing/).
:::

## What an install does

```mermaid title="From a source to the store"
flowchart TB
  S["source: folder, repository or release"] --> F["fetch"]
  F --> I["inspect the manifest"]
  I --> B{"declares a build?"}
  B -->|"yes"| R["run the command in a copy"]
  B -->|"no, or a release"| C["check the bundle"]
  R --> C
  C --> K["store the bundle, for new reviews"]
  K --> V["new reviews render with it"]
```

1. **Fetch.** Pinrail copies the folder, clones the repository at the ref, or downloads the release's bundle.
2. **Build, when declared.** A plugin written with a framework declares its build in the manifest, for example `"build": { "command": "npm ci && npm run build" }`. Pinrail runs it through the shell in a copy of the source, without `node_modules` or `.git`, shows the output as it runs, and keeps the log. A non-zero exit stops the install and shows the end of the log. The tools the command needs, such as `node` or `pnpm`, must be on your `PATH`.
3. **Check.** The manifest, the schemas and `view/index.html` must be valid. A plugin without `build` whose `view/index.html` is missing is refused, with the reason.
4. **Store.** Only the bundle is kept: `manifest.json`, `icon.svg`, `README.md`, `LICENSE` and the folders `schemas/`, `view/`, `templates/` and `samples/`, without hidden files. Everything else, such as sources, tests, fixtures and `node_modules/`, stays behind. The bundle is stored once, named by the hash of its files, and becomes the release new reviews use. Pinrail records where it came from.

## Plugin names

A plugin's full name is its publisher and its name, such as `forgeplane/list`. The publisher comes from where the plugin comes from, never from its manifest:

| Source | Publisher |
|---|---|
| The plugins that come with Pinrail | `forgeplane` |
| A repository or a release | The repository's owner: `github.com/acme/plugins` gives `acme`. A GitLab group path is joined with dots: `gitlab.com/acme/tools/review` gives `acme.tools`. |
| A folder, or a linked folder | `local` |

So two publishers can each have a plugin called `review`. Agents and commands may name a plugin by its name alone, such as `pinrail submit review`, when only one installed plugin has that name. When two do, the name alone is refused with both full names, and the full name, such as `acme/review`, picks one. Settings, link permissions and muted notifications belong to the full name.

## Versions and updates

Plugin versions are semantic, such as `"1.2.0"`. A review keeps the release it was submitted to, and always renders and validates with it. An update is for new reviews, so it never changes a review already made, and a decided review shows what you saw when you decided.

```mermaid title="Each review keeps its release"
flowchart LR
  a["1.0.0"] --> b["1.1.0"] --> c["2.0.0"]
  r1(["reviews made with 1.0.0"]) -.-> a
  r2(["reviews made with 1.1.0"]) -.-> b
  r3(["new reviews"]) -.-> c
```

- **A newer release** becomes the one new reviews use.
- **An older release** is refused unless you pass `--force`.
- **A release is kept** for as long as a review made with it is kept.

An update keeps the release it replaced for a week. Within that week, roll back from the plugin's details in *Settings › Plugins*, or from the command line:

```sh
pinrail plugins rollback review
```

New reviews then use the earlier release again. Reviews made in between keep the release they were made with.

Check for updates from the plugin's row in *Settings › Plugins*, or from the command line:

```sh
pinrail plugins update            # every installed plugin
pinrail plugins update review     # one plugin
```

An update that runs a build shows the build command first, in the app and on the command line, and runs it only when you confirm. On the command line, `--yes` confirms it in advance. When nobody can confirm, a plugin whose update runs a build is not updated, and the command reports it as failed.

A plugin installed from a repository at a tag or a commit, or from a release whose tag is only a version such as `v1.2.0`, is pinned. The update check reports that it is pinned and does not move it. A release whose tag names the plugin, such as `review-v1.2.0`, is not pinned: the update check follows newer releases of the same plugin.

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

The link takes the plugin's full name, so its existing reviews and new ones render with your folder. The plugin's row says that a local folder replaces it. Removing the link puts the published release back:

```sh
pinrail plugins remove review
```

## Removing a plugin

```sh
pinrail plugins remove ticket_triage
```

Removing a plugin stops new reviews from using it. Existing reviews keep the releases they were made with, so your history stays readable. The plugins that come with Pinrail cannot be removed.

## What runs on your machine

:::caution[A build runs code with your user permissions]
When you install a plugin from a source that needs a build, Pinrail runs the build command on your computer with your user's permissions. Installing a release does not run anything. Only install plugins from people and repositories whose code you would be willing to run. [What runs where](/docs/concepts/trust/#what-installing-a-plugin-runs) explains what each kind of installation runs.
:::

## Where plugins live

Under the app's data directory, `plugins/` holds three folders:

| Folder | Contents |
|---|---|
| `bundles/<hash>/` | The installed releases, each stored once and never changed. |
| `work/` | Scratch space an install uses and empties. |
| `logs/` | The last few build logs of each plugin. |
