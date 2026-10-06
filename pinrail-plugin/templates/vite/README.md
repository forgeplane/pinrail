# __TITLE__

A pinrail plugin: what the agent asks (`schemas/payload.schema.json`), what
the person answers (`schemas/decision.schema.json`), and the view that
turns one into the other (`src/`, built by Vite into `view/`). It starts as
one yes-or-no question; make it yours from there.

```sh
npm install
npx playwright install chromium       # once, for the tests
npm run watch                         # rebuilds view/ on every change…
npx pinrail-plugin dev                 # …and the view is in a browser, on samples/__NAME__.json, reloading
npm test                              # builds, then tests/ under the harness, no app needed
npm run check                         # what the app would say of the folder
pinrail plugins install . --link       # the app follows this folder; keep the watch running
```

Then, from an agent's session:

```sh
pinrail submit __NAME__ --sample --wait
```

`pinrail docs plugins/building` explains how a plugin works and how to
build one. An agent with Pinrail's skill has the same guide.

## Layout

```
manifest.json       name, version and title
src/                index.html and main.ts: the view, typed against pinrail-plugin/types
view/               the build; the app serves it in a sandboxed frame (not versioned)
schemas/            payload and decision, JSON Schema 2020-12
samples/            reviews to look at and test with: pinrail submit __NAME__ --sample
tests/              the Playwright spec the harness runs
```

The frame can load nothing from outside the folder: no fetch, no CDN. The
payload carries everything the view shows, and the build bundles the rest into `view/`.

## Releasing

Bump `version` in the manifest, tag `v<version>` and push the tag:
`.github/workflows/release.yml` builds, then attaches `__NAME__-<version>.zip`
to a GitHub release. Anyone downloads the zip and installs it with

```sh
pinrail plugins install ~/Downloads/__NAME__-<version>.zip
```

Pinrail installs a plugin as it is and runs nothing, so build the view
with `npm run build` before you install or link this folder.
