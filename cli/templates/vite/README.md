# __TITLE__

A Pinrail plugin: what an agent asks (`schemas/payload.schema.json`), what
the person answers (`schemas/decision.schema.json`), and the view between
them, written in TypeScript in `src/` and built by Vite into `view/`. It starts as
one yes-or-no question; make it yours from there.

```sh
npm install                         # once
npm run build                       # writes view/, the page the app serves
npm run watch                       # rebuilds view/ on every change
pinrail plugins check .             # what the app would refuse, and why
pinrail plugins install . --link    # the app follows this folder; keep the watch running
pinrail submit __NAME__ --sample    # a real review of its sample, in the inbox
pinrail docs plugins/building       # how a plugin works, and how to build one
```

Pinrail installs a plugin as it is and runs nothing, so build the view
before you install or link this folder. The view can load nothing from
outside the folder, so the build bundles everything it uses into `view/`.

## Layout

```
manifest.json       name, version, title, and when an agent should ask with it
src/                index.html and main.ts: the view, typed against pinrail-plugin.d.ts
view/               the build, which the app serves in a sandboxed frame (not versioned)
schemas/            payload and decision, JSON Schema 2020-12
samples/            reviews to look at and to try: pinrail submit __NAME__ --sample
```
