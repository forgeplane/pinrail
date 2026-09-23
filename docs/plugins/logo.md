---
title: Logo
description: "Candidate logo marks seen where they will live, a favourite picked, and changes asked for on parts of a mark."
sidebar:
  badge:
    text: Optional
    variant: default
---

<div class="pr-badges"><span class="pr-badge optional">Optional</span><span class="pr-badge plain">plugin: logo</span></div>

The logo plugin is for choosing between marks an agent drew: a logo, an app icon, a favicon. A mark that looks right at 128 px can fall apart at 16, or vanish in a one-colour menu bar, so each candidate is shown where it will actually live, in the brand's own colours, light and dark. You pick a favourite, keep or drop the rest, and can click any part of a mark to ask for a change to that part alone.

![The logo plugin: six candidate marks in the rail, one kept, one dropped, and the favourite open with its accent line selected for a change.](screenshot:logo "Six directions for a mark: one kept, one dropped, and the favourite with a change to its line being written.")

## When to use it

- **A new logo or mark**, explored in a few directions and narrowed round by round.
- **App icons and favicons**, which have to work at 16 and 32 px.
- **A refresh of an existing mark**, compared side by side with its variants.

## Install

```sh
pinrail plugins install github.com/forgeplane/pinrail/plugins/logo
```

## What you see

- **Every mark side by side** at 16 and 32 px, on the light and the dark palette, to compare at a glance.
- **The open mark large** on a grid, and on a ladder of sizes from 16 to 128 px.
- **The mark in place**: as a favicon in a browser tab, an app icon, in the Dock, in the macOS menu bar in one colour, and beside the wordmark with the tagline.
- **A verdict per mark**: *favourite*, *keep* or *drop*, with a note. There is one favourite at most; choosing another moves the old one to *keep*.
- **Changes to parts.** Click a rect, a circle or a path in the large mark and say what to change. The agent gets back the element's path in the SVG.
- **The previous round's verdicts**, beside each mark in the next round.

Keys: <kbd>j</kbd> / <kbd>k</kbd> next and previous mark, <kbd>f</kbd> favourite, <kbd>s</kbd> keep, <kbd>x</kbd> drop.

## Asking from your agent

```md title="AGENTS.md"
## Before settling on a mark

When you draw logo or icon candidates, don't pick one yourself. Submit them
to Pinrail as a `logo` review and wait:

1. Draw each mark as an SVG with a square viewBox. Use `currentColor` for
   the ink and `var(--accent)` for the accent, so it can be shown on every
   background.
2. Write the payload: the brand, its palette, and the marks, each with an
   `id`, a `name`, the `svg` and a line of `reasoning`.
3. Run: `pinrail submit logo --title "<brand> marks — round 1" --data marks.json --wait --format markdown`
4. Take the favourite forward. Apply each note, and each change asked for on
   a part of a mark to the element its `target` names. Drop what was
   dropped. Submit the next round with `--revises <id>`.
5. If the command exits 5, stop.
```

## What the agent sends

```json title="marks.json"
{
  "notes": "Round 1: six directions on one idea, the line the water leaves.",
  "brand": {
    "name": "Tidemark",
    "wordmark": "tide[mark]",
    "wordmark_font": "\"Avenir Next\", system-ui, sans-serif",
    "tagline": "Every change leaves a line."
  },
  "palette": {
    "light": { "background": "#eef3f4", "ink": "#12303a", "accent": "#0f8b8d" },
    "dark":  { "background": "#0f1c21", "ink": "#e3eef0", "accent": "#3cc1c3" }
  },
  "marks": [
    {
      "id": "M4",
      "name": "Post and line",
      "svg": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">…</svg>",
      "reasoning": "Two posts with the level strung between them. The most legible at 16 px."
    }
  ]
}
```

- **The wordmark** sets the part in `[brackets]` in the accent colour. The view cannot load fonts, so name one the viewer has, with fallbacks.
- **The palette** is optional; without one, a neutral pair is used.

:::caution[The SVG is drawn, never run]
Scripts, `<style>` elements, event handlers and links to anything outside the drawing are removed before a mark is shown.
:::

## What comes back

```json
{
  "decisions": [
    {
      "id": "M4",
      "action": "favorite",
      "note": "Pixel-tune it at 16 px",
      "comments": [
        {
          "target": "svg > rect:nth-of-type(3)",
          "tag": "rect",
          "markup": "<rect x=\"7\" y=\"10.5\" width=\"10\" height=\"3\" rx=\"1.5\" fill=\"var(--accent)\"></rect>",
          "note": "Lower the line a little, below the middle"
        }
      ]
    },
    { "id": "M1", "action": "keep" },
    { "id": "M2", "action": "drop", "note": "Too close to every other ring mark" }
  ],
  "undecided": ["M3", "M5", "M6"]
}
```

| Field | Meaning |
|---|---|
| `decisions[].action` | `favorite` to take forward, `keep` for the shortlist, `drop` to set aside. |
| `decisions[].note` | What to change about the mark, or why it was dropped. |
| `decisions[].comments[].target` | The element's path in the mark's SVG, such as `svg > g:nth-of-type(1) > rect:nth-of-type(2)`. |
| `decisions[].comments[].markup` | The element as it was drawn, so the agent can find it again if the path moved. |
| `undecided` | The marks given no verdict. Treat them as not chosen. |

With `--format markdown`, the agent reads the favourite first, then what was kept and dropped, each mark by name with its notes and the changes asked for on its parts.

## Reference

The plugin's manifest, and the schemas a payload and a decision are checked against, read from the plugin's own files.

![The Logo plugin's contract](contract:logo)
