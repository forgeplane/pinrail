---
title: 3D model
description: "Candidate 3D models on a stage to orbit, a favourite picked, and changes asked for on parts of a model."
sidebar:
  badge:
    text: Optional
    variant: default
---

<div class="pr-badges"><span class="pr-badge optional">Optional</span><span class="pr-badge plain">plugin: model</span></div>

The model plugin is for choosing between 3D models an agent made: a product, a part, a game asset, a scene. A model you can't turn around is hard to judge, so each candidate is on a stage you orbit and zoom, from set views and from the views the agent picked, under studio light, daylight or at night. You pick a favourite, keep or drop the rest, and can click any part of a model to ask for a change to that part alone.

![The model plugin: four desk lamps in the rail, one on the stage from three-quarters, with a comment pinned to its base and another being written on its shade.](screenshot:model "Four directions for a desk lamp: the favourite on the stage, one change pinned to its base and another being written on its shade.")

## When to use it

- **A product or a part**, explored in a few directions and narrowed round by round.
- **Game and scene assets**, where the silhouette, the size and the triangle count all matter.
- **A change to an existing model**, compared side by side with its variants.

## Install

```sh
pinrail plugins install github.com/forgeplane/pinrail/plugins/model
```

## What you see

- **Every model side by side**, as a still from the same angle, with its triangle and part counts.
- **The open model on a stage**: drag to orbit, scroll to zoom. The set views (¾, front, side, top) frame the whole model; the agent can add its own, such as the view from a chair.
- **Three lights**: studio, daylight and night, where what glows shows. A turntable, and the wireframe.
- **Its size** in real units, its triangles, parts and materials.
- **A verdict per model**: *favourite*, *keep* or *drop*, with a note. There is one favourite at most; choosing another moves the old one to *keep*.
- **Changes to parts.** Hover a part to see its name and material; click it, or pick it in the list of parts, and say what to change. The comment is pinned to the point you clicked, and *Show* brings back the view you wrote it from.
- **The previous round's verdicts**, beside each model in the next round.

Keys: <kbd>j</kbd> / <kbd>k</kbd> next and previous model, <kbd>1</kbd>–<kbd>9</kbd> the views, <kbd>t</kbd> turntable, <kbd>w</kbd> wireframe, <kbd>l</kbd> the next light, <kbd>f</kbd> favourite, <kbd>s</kbd> keep, <kbd>x</kbd> drop.

## Asking from your agent

```md title="AGENTS.md"
## Before settling on a model

When you make candidate 3D models, don't pick one yourself. Submit them to
Pinrail as a `model` review and wait:

1. Export each model as a GLB file with its textures embedded, and name its
   nodes and materials: comments come back by those names.
2. Write the payload: the subject and its units, and the models, each with
   an `id`, a `name`, its file as `{"$artifact": "<file name>"}`, a line of
   `reasoning`, and any camera `views` worth a look.
3. Run: `pinrail submit model --title "<subject> — round 1" --data models.json --artifact <file>.glb … --wait --format markdown`,
   one `--artifact` per model file.
4. Take the favourite forward. Apply each note, and each change asked for on
   a part to the node its `target` names. Drop what was dropped. Submit the
   next round with `--revises <id>`.
5. If the command exits 5, stop.
```

## What the agent sends

```sh
pinrail submit model --title "Halden desk lamp — round 1" --data models.json \
  --artifact out/pivot.glb --wait --format markdown
```

```json title="models.json"
{
  "notes": "Round 1: four directions for the Halden desk lamp.",
  "subject": { "name": "Halden desk lamp", "units": "m" },
  "models": [
    {
      "id": "L1",
      "name": "Pivot",
      "file": { "$artifact": "pivot.glb" },
      "reasoning": "The classic two-arm task lamp. Brass only where it moves.",
      "views": [{ "name": "Seated", "position": [0.55, 0.32, 0.55], "target": [0, 0.2, 0] }]
    }
  ]
}
```

- **A model** is a file sent with `--artifact` and named in `file`: a `.glb`, or a `.gltf` with everything embedded. Textures and buffers must be inside it, since the view fetches nothing else. A small model can go inline instead, as three.js JSON in `object` (what `Object3D.toJSON()` returns).
- **Units** say what one unit is, for the sizes shown; the metre, glTF's own, is the default. Set `up` to `z` for most CAD exports.
- **Views** are cameras in the model's own coordinates; `target` defaults to the model's centre.

:::note[Size]
A file may be up to 50 MB, and a round may carry 12. The app stores each file once, so a new round only uploads the models that changed.
:::

## What comes back

```json
{
  "decisions": [
    {
      "id": "L1",
      "action": "favorite",
      "note": "Warmer overall",
      "comments": [
        {
          "target": "Lamp > Lower arm > Upper arm > Head > Shade",
          "name": "Shade",
          "material": "Powder coat",
          "point": [0.1712, 0.3391, 0.0189],
          "view": { "position": [0.55, 0.32, 0.55], "target": [0, 0.2, 0] },
          "note": "Wider and shallower, so the bulb is hidden from the chair"
        }
      ]
    },
    { "id": "L3", "action": "keep" },
    { "id": "L2", "action": "drop", "note": "A lamp that cannot be aimed is not a desk lamp" }
  ],
  "undecided": ["L4"]
}
```

| Field | Meaning |
|---|---|
| `decisions[].action` | `favorite` to take forward, `keep` for the shortlist, `drop` to set aside. |
| `decisions[].note` | What to change about the model, or why it was dropped. |
| `decisions[].comments[].target` | The part's path by node names, such as `Lamp > Head > Shade`. |
| `decisions[].comments[].material` | The name of the material where the comment was pinned. |
| `decisions[].comments[].point` | Where on the part it was pinned, in the model's own coordinates. |
| `decisions[].comments[].view` | The camera it was seen through, in the same coordinates. |
| `undecided` | The models given no verdict. Treat them as not chosen. |

With `--format markdown`, the agent reads the favourite first, then what was kept and dropped, each model by name with its notes and the changes asked for on its parts.
