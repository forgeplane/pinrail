---
title: Frameworks
summary: "Views built with React, Vue, Svelte or TypeScript, created with the plugin SDK from a checkout of the Pinrail repository."
menu: []
---
# Frameworks

A view can be built with React, Vue, Svelte or TypeScript, as long as the
build writes an HTML page and its files into the plugin folder. The plugin
SDK's `create` command writes a working plugin for each of these, with a
Vite build, a test and a release workflow. The SDK is not published to npm,
so run it from a checkout of the Pinrail repository:

```sh
git clone https://github.com/forgeplane/pinrail
node pinrail/pinrail-plugin/bin/pinrail-plugin.mjs create <name> --template react   # or vite, vue, svelte
```

The new plugin's `package.json` takes the SDK from its GitHub release. To
use your checkout instead, add `--sdk file:<path to pinrail/pinrail-plugin>`.

What the app needs from a built view:

- The build writes the page to `view/index.html`, with its scripts and
  styles beside it in `view/`.
- `build` in the manifest is the command an installation runs, such as
  `npm ci && npm run build`. A plugin installed with `--link` is served as
  it is, so build it yourself first.
- The build gets no input and is stopped after 15 minutes, so every step
  must finish by itself.
- The build refers to its files with relative paths. With Vite, set
  `base: "./"`.
- The page loads `/sdk/v1/pinrail-plugin.js` and its stylesheet with tags
  instead of bundling them.
- Everything else, including the framework, fonts and images, is bundled,
  because the view cannot load anything from the network.
