---
title: Image review
description: "Generated images or illustrations, each large on a stage to zoom and pan, with a verdict on each and boxes or pins on the parts to change."
sidebar:
  badge:
    text: Optional
    variant: default
---

<div class="pr-badges"><span class="pr-badge optional">Optional</span><span class="pr-badge plain">plugin: image</span></div>

The image review plugin shows a round of images an agent generated, such as illustrations, product shots or icons, and asks for a verdict on each. You look at every image closely, choose the one to take forward, and mark the parts to change on the image itself. The agent gets each mark in coordinates an image-editing or inpainting step can use.

![The image review plugin: a paper plane illustration chosen as the favourite, with a box on a cloud and a pin on the plane's nose, and a note to the agent.](screenshot:image "The favourite, with a box and a pin on the parts to change and a note on the image as a whole.")

## When to use it

- **Illustrations and spot art**, where you choose one direction from several.
- **Product shots and marketing images**, where details need fixing before they ship.
- **Icon sets and app artwork**, checked on light and dark backgrounds.

## Install

The plugin comes with the app. Install it in *Settings › Plugins*, or from the command line:

```sh
pinrail plugins install image
```

## What you see

- **The round's notes** from the agent, across the top, in view while you work.
- **A list of the images**, each with its size, format and verdict. A review of a single image shows the image alone.
- **Each image large on a stage**, to zoom with the scroll wheel or <kbd>=</kbd> and <kbd>-</kbd>, and to pan by dragging with the space bar held. *Fit* and *1:1* return to the whole image and to its own pixels.
- **A backdrop** behind transparent pixels: a checkerboard, light or dark. The agent picks the first one, and <kbd>g</kbd> switches.
- **The agent's reasoning**, the prompt it used, and how the image was made, such as the model and the seed.
- **Favourite**, **Keep** or **Drop** for each image, and a note on it if you add one. There is at most one favourite. A review of a single image offers only *Keep* and *Drop*.
- **Regions to change.** Drag a box over part of an image, or click a spot, and say what should change there.
- **The previous round.** When the agent sends a new round, <kbd>c</kbd> switches to the same image from the round before, with the regions you marked on it.

Press <kbd>?</kbd> for the keys.

To see it before any agent asks with it, send its sample: `pinrail submit image --sample`, or **Send a sample** in its details in *Settings › Plugins*.

## Asking from your agent

```md title="AGENTS.md"
## Before using generated images

When you generate images for me to choose from, submit them to Pinrail with
the `image` plugin and wait for my decision:

1. Write the payload: a `notes` line on what this round is, and one entry per
   image with a stable `id`, a `name`, its `file`, and your `reasoning`.
2. Run: `pinrail submit image --title "<what the images are for>" --data images.json --attach <each file> --wait`
3. Take the `favorite` forward. Keep the `keep` images as alternatives, and
   drop the rest.
4. Apply each region's `note` to the area it marks, and each image's `note`
   to the image as a whole, then submit the next round with `--revises <id>`,
   keeping each image's `id`.
5. If the command exits 5, stop and use none of the images.
```

## What the agent sends

```json title="images.json"
{
  "notes": "Round 1 of the empty inbox illustration: four directions.",
  "subject": { "name": "Tern empty-inbox illustration", "use": "320 px wide, light and dark" },
  "backdrop": "checker",
  "images": [
    { "id": "A", "name": "Paper plane",
      "file": { "$attachment": "paper-plane.png" },
      "reasoning": "Mail as something that **flies off and is done**.",
      "prompt": "Flat vector spot illustration, a paper plane looping over hills",
      "details": { "model": "flux-1.1-pro", "seed": 48211 } }
  ]
}
```

- Each image is a file sent with `--attach` and named in `file`: PNG, JPEG, WebP or SVG, up to 25 MB each and 16 in a round.
- `notes` and `reasoning` are markdown. `details` are short facts, shown as chips.
- `backdrop` is the first backdrop behind transparent pixels: `checker`, `light` or `dark`.
- `favorite` says whether the person can mark one image as their favourite. The default is `true` when the review has more than one image, and `false` when it has only one. Set it to `false` when each image only needs to be kept or dropped.

## What comes back

```json
{
  "decisions": [
    { "id": "A", "action": "favorite", "note": "Keep the palette exactly as it is",
      "size": { "width": 960, "height": 720 },
      "regions": [
        { "shape": "box", "x": 0.9083, "y": 0.4111, "width": 0.0917, "height": 0.1389,
          "px": { "x": 872, "y": 296, "width": 88, "height": 100 },
          "note": "Remove this cloud: it is cut off by the edge" }
      ] },
    { "id": "B", "action": "keep" },
    { "id": "C", "action": "drop", "note": "Too dark for the light theme" }
  ],
  "undecided": ["D"]
}
```

| Field | What the agent does with it |
|---|---|
| `decisions` | Takes the `favorite` forward, keeps the `keep` images as alternatives, and sets the `drop` images aside. A `note` applies to the image as a whole. |
| `regions` | Changes each marked area as its `note` says. `x`, `y`, `width` and `height` are fractions of the image from its top left, and `px` is the same area in the image's pixels. A box can serve as an inpainting mask. A point marks a spot. |
| `undecided` | Images you gave no verdict. An image with regions and no verdict counts as kept. |

## Reference

The plugin's manifest, and the schemas a payload and a decision are checked against, read from the plugin's own files.

![The Image review plugin's contract](contract:image)
