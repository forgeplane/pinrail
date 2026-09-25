---
title: Frameworks
summary: A view in React, Vue, Svelte or TypeScript, built into view/.
menu: []
---
# Frameworks

A view with a build starts from the npm package, which needs Node 22 or
later:

```sh
npx @forgeplane/pinrail-plugin create <name> --template react   # or vue, svelte, vite
```

- Sources are in `src/`; the build writes `view/`, which the app serves.
  `npm run watch` rebuilds it as you edit.
- The manifest's `build` command builds it when the plugin is installed
  from its sources.
- The build refers to its files relatively: with Vite, `base: "./"`.
- Load the SDK from the app with tags in `src/index.html`; never bundle
  it. Its types: `@forgeplane/pinrail-plugin/types`.
- Bundle everything else: the frame loads nothing from the network.
- `npm test` runs the plugin's tests under the package's harness, without
  the app.
