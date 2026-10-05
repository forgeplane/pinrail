# image

An agent brings a round of generated images or illustrations; you look at
each one closely and mark what to change on the image itself:

- every image in the rail, with its size and format;
- the chosen image large on a stage, to zoom (scroll, `=` / `-`, *Fit*,
  *1:1*) and pan (space-drag, the hand tool, or the arrows), over a
  checkerboard, white or black for transparent pixels;
- the agent's reasoning, the prompt it used, and how it was made (model,
  seed, steps), when it sends them;
- in a later round, the previous round's image of the same id, with the
  regions asked for on it, one key away (`c`).

For each image you say **favourite**, **keep** or **drop**, and can add a
note. There is one favourite at most: choosing another moves the old one to
*keep*. A round can leave out the favourite (see `favorite` below). Drag
a box over part of an image, or click a spot, and say what to change there:
*remove this*, *the hands look wrong*. An image with regions and no verdict
counts as kept.

PNG, JPEG, WebP and SVG are accepted. An SVG is shown as a picture: its
scripts, event handlers and links never run.

## Asking

```sh
pinrail plugins install image
pinrail submit image --title "Empty inbox illustration — round 1" --data images.json \
  --attach out/paper-plane.png --attach out/tern.svg --wait --format markdown
```

## Payload

```json
{
  "notes": "markdown: what this round is and what to look at",
  "subject": { "name": "Tern empty-inbox illustration", "use": "320 px wide, light and dark" },
  "backdrop": "checker",
  "images": [
    { "id": "A", "name": "Paper plane",
      "file": { "$attachment": "paper-plane.png" },
      "reasoning": "markdown: the idea, and what to look for",
      "prompt": "Flat vector spot illustration, a paper plane looping over hills…",
      "details": { "model": "flux-1.1-pro", "seed": 48211 },
      "alt": "A paper plane loops over teal hills." }
  ]
}
```

- **An image** is a file sent with `--attach` and named in `file`: up to
  25 MB each and 16 a round.
- **`details`** are short facts shown as chips; **`prompt`** is shown as it is.
- **`backdrop`** is what shows through transparent pixels at first:
  `checker`, `light` or `dark`.
- **`favorite`** says whether the person can mark one image as their
  favourite. The default is `true` when the review has more than one image,
  and `false` when it has only one. Set it to `false` when each image only
  needs to be kept or dropped.

## Decision

```json
{
  "decisions": [
    { "id": "A", "action": "favorite", "note": "Keep the palette exactly as it is",
      "size": { "width": 960, "height": 720 },
      "regions": [
        { "shape": "box", "x": 0.9083, "y": 0.4111, "width": 0.0917, "height": 0.1389,
          "px": { "x": 872, "y": 296, "width": 88, "height": 100 },
          "note": "Remove this cloud: it is cut off by the edge" },
        { "shape": "point", "x": 0.875, "y": 0.2986, "px": { "x": 840, "y": 215 },
          "note": "Tilt the nose up a little, towards the sun" }
      ] },
    { "id": "B", "action": "keep" },
    { "id": "C", "action": "drop", "note": "Too dark for the light theme" }
  ],
  "undecided": ["D"]
}
```

`favorite` is the image to take forward, `keep` stays on the shortlist, `drop`
is set aside. A region's `x`, `y`, `width` and `height` are fractions of the
image from its top left; `px` is the same region in the image's own pixels,
and `size` is the image's size those pixels are measured in. For an SVG, `px`
and `size` are in its viewBox units, so a region maps onto its drawing. A box
is a ready mask for an inpainting step: fill `px.x, px.y, px.width,
px.height` and describe the change with `note`. A point is a spot to change,
too small to be a mask on its own. For the next round, submit with
`--revises <id>` and keep each image's `id`: the view shows its verdict last
time, and `c` flips to last round's image.

## Keys

`j` / `k` next and previous image · `f` favourite · `s` keep · `x` drop ·
`p` a pin at the centre of the view · `b` a box around what is in view ·
`=` / `-` zoom · `0` fit · `1` actual pixels · arrows pan · `h` drag to pan ·
`g` backdrop · `c` the previous round.

## Developing

`node scripts/fixture.mjs` draws the Tern illustrations again into
`fixtures/tern/`. `mise run dev:plugin plugins/image` opens the view on its
fixtures without the app, and `mise run test:plugins` runs its tests with
the other plugins'.
