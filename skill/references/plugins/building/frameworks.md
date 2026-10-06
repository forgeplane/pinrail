# Frameworks

A view can be built with React, Vue, Svelte or TypeScript, as long as the
build writes an HTML page and its files into the plugin folder.
`pinrail plugins new` writes a working plugin in React or TypeScript, with
a Vite build:

```sh
pinrail plugins new <name> --template react --dir <path>   # or --template vite, in TypeScript
```

For Vue or Svelte, start from the TypeScript plugin, and follow the
matching version of the Ship it? example in `docs/examples/ship-it/` of
the Pinrail repository.

The view is in `src/`, and takes the SDK's types from
`pinrail-plugin.d.ts` in the folder. Run `npm install` once, then
`npm run build`, or `npm run watch` while you work. The same steps follow
as for a plain plugin, with these differences for the view:

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
