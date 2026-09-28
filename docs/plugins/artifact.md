---
title: Artifact
description: "An HTML page an agent designed, commented on element by element, with selectors sent back to the agent."
sidebar:
  badge:
    text: Optional
    variant: default
---

<div class="pr-badges"><span class="pr-badge optional">Optional</span><span class="pr-badge plain">plugin: artifact</span></div>

The artifact plugin is for reviewing something an agent designed: a landing page, a mockup, an email template, a dashboard. You review it the way you would in browser developer tools: turn on *Select*, click an element, and say what should change. The agent gets back a CSS selector for each comment, something it can act on, rather than a paragraph it has to interpret.

![The artifact plugin: a landing page with two comments pinned to elements and a third being written on a feature card.](screenshot:artifact "Two comments pinned, and a third being written on the element just picked.")

## When to use it

- **Pages and mockups** an agent builds from a brief.
- **Email templates and newsletters**, checked at desktop and phone widths.
- **Dashboards and reports** rendered as HTML.
- **Rounds of design feedback**, with each round's comments shown beside the next version.

## Install

```sh
pinrail plugins install github.com/forgeplane/pinrail/plugins/artifact
```

:::note
This plugin's view is built with Vite, so installing it from a folder or a repository runs `npm ci && npm run build` on your machine, and Pinrail shows you the command first. Installing from a release needs no build.
:::

## What you see

- **The page**, at desktop, tablet or phone width.
- **Select mode.** Hover to see an element outlined with its tag, click to comment on it.
- **Comments** pinned to their elements and listed in a panel, each marked *change*, *question* or *praise*, and editable until you hand over.
- **A verdict.** Any comment means *request changes*, unless you set the verdict yourself.
- **The previous round's comments**, beside the new version of the page.

To see it before any agent asks with it, send its sample: `pinrail submit artifact --sample`, or **Send a sample** in its details in *Settings › Plugins*.

## Asking from your agent

```md title="AGENTS.md"
## Before shipping a page or template

When you finish an HTML page, mockup or template, don't ship it. Submit it
to Pinrail as an `artifact` and wait:

1. Make the HTML self-contained: styles in `<style>` elements, images and
   fonts as data URIs. Nothing external loads.
2. Write the payload: `{ "title": "…", "notes": "what to look at", "viewport": "desktop", "html": "<!doctype html>…" }`,
   or, for a page already in a file, `"file": {"$attachment": "page.html"}` in place of `html`.
3. Run: `pinrail submit artifact --title "<page> — round 1" --data page.json --wait`,
   adding `--attach page.html` when the payload names a file.
4. If the verdict is `approve`, ship it. If it is `revise`, apply each
   comment to the element its `selector` names, then submit the new version
   with `--revises <id>`.
5. If the command exits 5, stop.
```

## What the agent sends

```json title="page.json"
{
  "title": "Ledgerly landing page",
  "notes": "First pass from the brief. Look at the headline, the feature cards and the pricing block.",
  "viewport": "desktop",
  "html": "<!doctype html><html><head><style>…</style></head><body>…</body></html>"
}
```

A page that is already in a file goes beside the payload instead of inside it. Name it as `file` and send it with `--attach`. The review takes one `.html` file of up to 10 MB.

```sh
pinrail submit artifact --title "Ledgerly landing page" --data page.json --attach landing.html --wait
```

```json title="page.json"
{ "title": "Ledgerly landing page", "file": { "$attachment": "landing.html" } }
```

:::caution[Self-contained HTML only]
The page renders in isolation. External stylesheets, scripts and images do not load, links and forms go nowhere, and scripts do not run. Inline every style and embed images and fonts as data URIs.
:::

## What comes back

```json
{
  "verdict": "revise",
  "comments": [
    {
      "id": "c_k2n4x9ab",
      "selector": "#hero > h1",
      "tag": "h1",
      "kind": "change",
      "text": "Say what it does, not a slogan.",
      "snippet": "Bookkeeping that closes itself",
      "html": "<h1>Bookkeeping that closes itself</h1>"
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `verdict` | `approve` or `revise`. The app shows it in your history as *approved* or *changes requested*. |
| `comments[].selector` | Unique in the page as reviewed: the element's id, or a path from the nearest ancestor with one, such as `#features > div:nth-of-type(2) > h3`. |
| `comments[].kind` | `change`, `question` or `praise`. |
| `comments[].snippet`, `comments[].html` | The element's text and markup, so the agent can find it even after moving things around. |

## Reference

The plugin's manifest, and the schemas a payload and a decision are checked against, read from the plugin's own files.

![The Artifact plugin's contract](contract:artifact)
