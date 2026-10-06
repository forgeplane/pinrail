# Frameworks

A view can be built with React, Vue, Svelte or TypeScript, as long as the
build writes an HTML page and its files into the plugin folder. The plugin
SDK's `create` command writes a working plugin in React or TypeScript,
with a Vite build, a test and a release workflow. For Vue or Svelte, start
from the TypeScript plugin, and follow the matching version of the Ship it?
example in `docs/examples/ship-it/`.

The SDK is not published to npm, so run it from a checkout of the Pinrail
repository:

```sh
git clone https://github.com/forgeplane/pinrail
node pinrail/pinrail-plugin/bin/pinrail-plugin.mjs create <name> --template react   # or vite
```

The new plugin's `package.json` takes the SDK from its GitHub release. To
use your checkout instead, add `--sdk file:<path to pinrail/pinrail-plugin>`.

The folder has the same files as one from `pinrail plugins new`, and the
same steps follow, with these differences for the view:

- The build writes the page to `view/index.html`, with its scripts and
  styles beside it in `view/`.
- Pinrail installs a plugin as it is and runs nothing, so run the build,
  such as `npm run build`, before you install, link or zip the folder.
- The build refers to its files by relative path. With Vite, set
  `base: "./"`.
- The page loads `/sdk/v1/pinrail-plugin.js` and its stylesheet with tags,
  rather than bundling them.
- The build bundles everything else, including the framework, fonts and
  images, because the view cannot load anything from the network.
