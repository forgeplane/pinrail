# Developing the SDK

This folder holds the plugin SDK and the `pinrail-sdk` package. This page
is for working on them in the Pinrail repository; [README.md](README.md)
is the package's own, for a plugin's author.

## What is in this folder

| Path | What it is | In the package |
|---|---|---|
| `src/` | The SDK a view loads: `pinrail-plugin.js`, `pinrail-plugin.css`, `tokens.css` and `markdown.js` | yes |
| `host/` | The app's side of the protocol, which every host of a view runs: the app's window, its preview page, the dev shell and the harness | yes |
| `schemas/` | The JSON Schemas of a plugin's manifest and of a reference to an attached file | yes |
| `types.d.ts` | The protocol and the manifest as TypeScript types | yes |
| `shell/` | The dev shell: its server, its page, and the script that lets *Select* pick a part of the view | yes |
| `testing/` | The test harness, exported as `pinrail-sdk/testing` | yes |
| `bin/` | The `pinrail-sdk` command | yes |
| `lib/` | What the dev shell and the harness share. `paths.cjs` finds the package's files and builds `markdown.js`, for the app's build as well. `schemas.cjs` checks a value against a plugin's schema as the app does. `attachments.cjs` lists a fixture's files as the app does. | yes |
| `dist/` | `markdown.js` with its parser bundled, built when the package is packed; not versioned | yes |
| `scripts/` | The build of `dist/markdown.js` | no |
| `test/` | The package's own tests | no |

`npm pack` shows exactly what the package carries: the `files` list in
`package.json` decides it.

## Where the SDK's files go

`src/` is the only copy of the SDK. Nothing else in the repository holds
one, and each user of the SDK takes it from here:

- **The app.** Its build, `desktop/app/scripts/build-sdk.mjs`, run by
  `npm run dev` and `npm run build`, copies `src/` into
  `desktop/app/sdk/v1/`, which is not versioned. It also bundles the
  Markdown parser into `markdown.js`, and adds the typeface the stylesheet
  uses. The app carries that folder and serves it at `/sdk/v1/`.
- **The dev shell and the harness** serve `src/` itself, at the same
  paths, so a view behaves in them as in the app.
- **The npm package** carries `src/`, with `markdown.js` bundled into
  `dist/` when it is packed.

The other shared files are used in the same way. The app bundles
`host/host.js` into its window and embeds it in its preview page. The
app's plugin checks and the CLI embed `schemas/manifest.schema.json` when
they are built, and the app's checks compare a payload schema's file
fields with `schemas/attachment.schema.json`. The CLI writes `types.d.ts`
into every new plugin as `pinrail-plugin.d.ts`, and takes the package's
version from `package.json`, for the dependency it writes and the dev
shell command the skill gives.

## The package's tests

```sh
npm test    # the unit tests, then the browser tests
```

- **Unit tests, under Node:** the SDK's script against a fake host
  (`plugin.test.cjs`), the app's side of the protocol (`host.test.mjs`),
  the types, which must accept a correct view and refuse a wrong one
  (`types.test.cjs`, `types.compile.test.cjs`), and the package as npm
  packs it (`package.test.mjs`).
- **Browser tests, with Playwright:** the SDK in a real view's frame,
  which has an opaque origin: connecting, keys, attached files, Markdown
  and the stylesheet. Then the harness, which must hold a plugin to what
  the app would; the conformance view, which every host must answer the
  same way; and the dev shell.

The app's own end-to-end tests run the same conformance view against the
app, so the app and the harness are held to one standard.

## Using the package from this repository

Until a version is on npm, take the package from the checkout:

```sh
mise run dev:plugin plugins/hello                                   # the dev shell, on a plugin here
pinrail plugins new ticket_triage --playwright --sdk "file:$PWD/sdk"  # a plugin whose tests use this folder
```

The official plugins in `plugins/` and the examples in `docs/examples/`
depend on the folder the same way, with a `file:` dependency.

## Releasing

A tag `sdk-v<version>` releases the package: `.github/workflows/sdk-release.yml`
runs its tests and attaches the tarball to a GitHub release. The package
is published to npm as [`pinrail-sdk`](https://www.npmjs.com/package/pinrail-sdk),
which is where the plugins that `pinrail plugins new --playwright` writes
take it from.
