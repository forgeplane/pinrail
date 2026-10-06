# __TITLE__

A pinrail plugin: what the agent asks (`schemas/payload.schema.json`), what
the person answers (`schemas/decision.schema.json`), and the view that
turns one into the other (`view/index.html` and `view/view.js`). It starts as one yes-or-no
question; make it yours from there.

```sh
npm install
npx playwright install chromium       # once, for the tests
npx pinrail-plugin dev                 # the view in a browser, on samples/__NAME__.json, reloading on change
npm test                              # tests/ under the harness, no app needed
npm run check                         # what the app would say of the folder
pinrail plugins install . --link       # the app follows this folder as you change it
```

Then, from an agent's session:

```sh
pinrail submit __NAME__ --sample --wait
```

`pinrail docs plugins/building` explains how a plugin works and how to
build one. An agent with Pinrail's skill has the same guide.

## Layout

```
manifest.json       name, version, title, and when an agent should ask with it
view/index.html     the view the app serves, in a sandboxed frame
view/view.js        its script, checked against pinrail-plugin.d.ts
samples/            reviews to look at and test with: pinrail submit __NAME__ --sample
schemas/            payload and decision, JSON Schema 2020-12
tests/              the Playwright spec the harness runs
```

The frame can load nothing from outside the folder: no fetch, no CDN. The
payload carries everything the view shows.

## Releasing

Bump `version` in the manifest, tag `v<version>` and push the tag:
`.github/workflows/release.yml` attaches `__NAME__-<version>.zip` to a GitHub
release. Anyone downloads the zip and installs it with

```sh
pinrail plugins install ~/Downloads/__NAME__-<version>.zip
```
