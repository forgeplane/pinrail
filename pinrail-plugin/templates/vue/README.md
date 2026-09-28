# __TITLE__

A pinrail plugin: what the agent asks (`schemas/payload.schema.json`), what
the person answers (`schemas/decision.schema.json`), and the view that
turns one into the other (`src/`, in Vue, built by Vite into `view/`). It starts as
one yes-or-no question with a comment; make it yours from there.

```sh
npm install
npx playwright install chromium       # once, for the tests
npm run watch                         # rebuilds view/ on every change…
npx pinrail-plugin dev                 # …and the view is in a browser, on fixtures/basic.json, reloading
npm test                              # builds, then tests/ under the harness, no app needed
npm run check                         # what the app would say of the folder
pinrail plugins install . --link       # the app serves this folder live; keep the watch running
```

Then, from an agent's session:

```sh
pinrail submit __NAME__ --title "Push the branch?" --data example.json --wait
```

## Layout

```
manifest.json       name, version, the schemas, the entry and the build, by path
src/                index.html, main.ts and App.vue: the view in Vue, typed against pinrail-plugin/types
view/               the build; the app serves it in a sandboxed frame (not versioned)
schemas/            payload and decision, JSON Schema 2020-12
fixtures/           payloads to develop and test with
tests/              the Playwright spec the harness runs
```

The frame can load nothing from outside the folder: no fetch, no CDN. The
payload carries everything the view shows, and the build bundles the rest into `view/`.

## Releasing

Bump `version` in the manifest, tag `v<version>` and push the tag:
`.github/workflows/release.yml` builds, then attaches `__NAME__-<version>.zip`
to a GitHub release, and anyone installs it with

```sh
pinrail plugins install https://github.com/<owner>/<repo>/releases/latest
```

Installing from the source folder or its git URL runs the manifest's
`build.command` instead, on the person's machine.
