---
title: Plugins
description: "The plugins that come with Pinrail, what each is for, and how to pick one for the decision your agent needs."
---

Every review uses a plugin, and the plugin decides what the person sees and what the agent gets back. Five official plugins come with the app. The setup installs the two it recommends, and you install the others when your agents need them.

## Which plugin to use

| When your agent needs a person to… | Use | |
|---|---|---|
| Accept or reject a set of findings, tasks or proposed actions | [List](/docs/plugins/list/) | <span class="pr-badge recommended">Recommended</span> |
| Answer questions before it goes on: choices, preferences, confirmations | [Feedback](/docs/plugins/feedback/) | <span class="pr-badge recommended">Recommended</span> |
| Review the comments it wants to post on a pull or merge request | [Code review](/docs/plugins/code-review/) | <span class="pr-badge optional">Optional</span> |
| Choose between images or illustrations it generated, and mark what to change on them | [Image review](/docs/plugins/image/) | <span class="pr-badge optional">Optional</span> |
| Read a plan, spec or document it wrote, and ask for changes or explanations | [Markdown review](/docs/plugins/markdown/) | <span class="pr-badge optional">Optional</span> |
| Edit emails, choose calendar slots, or review designed pages, logos or 3D models | [More plugins](/docs/plugins/more/) | |

When nothing fits exactly, start with **List**. Almost any batch of proposed actions reads well as grouped items with a verdict each, and the setup installs it. When the decision needs a view of its own, [build a plugin](/docs/building/writing/).

## Installing and updating

<span class="pr-badge recommended">Recommended</span> plugins are the ones the setup installs. <span class="pr-badge optional">Optional</span> plugins come with the app too, and you install them when you need them. Install either kind in *Settings › Plugins*, where the *Install* field lists the plugins not yet installed, or from the command line:

```sh
pinrail plugins install markdown
```

A newer version of a plugin arrives with an app update. *Settings › Plugins* then shows *Update* on the plugin's row, and nothing changes until you choose it. Any plugin can be removed. See [Installing plugins](/docs/using/installing-plugins/).

## How every plugin page is laid out

Each page answers the same questions, in the same order:

1. **What it is for**, with the situations it suits.
2. **What you see**: the view, and what you can do in it.
3. **Asking from your agent**: instructions to paste into your agent's instructions file, so it knows when and how to ask.
4. **What the agent sends** and **what comes back**: the payload and the decision, with examples.

:::tip[Start with the instructions]
The fastest way to try a plugin is to paste its instructions into your agent's `AGENTS.md` or `CLAUDE.md` and let the agent do the rest. [Instructing an agent](/docs/agents/instructing/) explains where instructions go and how to write your own.
:::
