---
title: Plugins
description: "The plugins that come with Pinrail, what each is for, and how to pick one for the decision your agent needs."
---

Every review uses a plugin, and the plugin decides what the person sees and what the agent gets back. Pinrail comes with two built-in plugins that are always available, and six optional ones that you install when your agents need them.

## Which plugin to use

| When your agent needs a person to… | Use | |
|---|---|---|
| Accept or reject a set of findings, tasks or proposed actions | [List](/docs/plugins/list/) | <span class="pr-badge built-in">Built in</span> |
| Answer questions before it goes on: choices, preferences, confirmations | [Feedback](/docs/plugins/feedback/) | <span class="pr-badge built-in">Built in</span> |
| Review the comments it wants to post on a pull or merge request | [Code review](/docs/plugins/code-review/) | <span class="pr-badge optional">Optional</span> |
| Read, edit and approve emails before they are sent | [Email](/docs/plugins/email/) | <span class="pr-badge optional">Optional</span> |
| Comment on a page, mockup or template it designed | [Artifact](/docs/plugins/artifact/) | <span class="pr-badge optional">Optional</span> |
| Choose times for appointments, meetings or interviews it found | [Calendar](/docs/plugins/calendar/) | <span class="pr-badge optional">Optional</span> |
| Choose between logo marks, app icons or favicons it drew | [Logo](/docs/plugins/logo/) | <span class="pr-badge optional">Optional</span> |
| Choose between 3D models it made, and ask for changes to their parts | [3D model](/docs/plugins/model/) | <span class="pr-badge optional">Optional</span> |

When nothing fits exactly, start with **List**. Almost any batch of proposed actions reads well as grouped items with a verdict each, and your agent can use it today, with nothing to install. When the decision needs a view of its own, [build a plugin](/docs/building/writing/).

## Built in and optional

<span class="pr-badge built-in">Built in</span> plugins ship inside the app. They are always installed, always at the version that matches your app, and cannot be removed or replaced.

<span class="pr-badge optional">Optional</span> plugins are installed one at a time, from the app or the command line. Download a plugin's zip from [the official plugins' releases page](https://github.com/forgeplane/pinrail-plugins/releases), then install it:

```sh
pinrail plugins install ~/Downloads/<name>-<version>.zip
```

To upgrade a plugin, install the zip of its new version the same way. See [Installing plugins](/docs/using/installing-plugins/).

## How every plugin page is laid out

Each page answers the same questions, in the same order:

1. **What it is for**, with the situations it suits.
2. **What you see**: the view, and what you can do in it.
3. **Asking from your agent**: instructions to paste into your agent's instructions file, so it knows when and how to ask.
4. **What the agent sends** and **what comes back**: the payload and the decision, with examples.

:::tip[Start with the instructions]
The fastest way to try a plugin is to paste its instructions into your agent's `AGENTS.md` or `CLAUDE.md` and let the agent do the rest. [Instructing an agent](/docs/agents/instructing/) explains where instructions go and how to write your own.
:::
