# Changelog

The package's version is the SDK's, and its major is the protocol's:
`1.x` speaks protocol 1, served by the app at `/sdk/v1`. A breaking
change to the protocol is a new major and a new path.

## 1.0.0

The first release of the plugin SDK, `pinrail-sdk`. It
provides:

- The script and stylesheet that plugin views load from the app at
  `/sdk/v1`: the messages with the app, drafts, the hand-over, Markdown
  rendering, icons, links, attached files and forwarded shortcuts.
- `pinrail-sdk dev`, which runs a view in a browser without the app.
- `pinrail-sdk/testing`, the test harness: `mountPlugin`,
  `fixture` and `reviewFrom`.
- `pinrail-sdk/types`, the manifest, the review, the
  messages and `window.Pinrail` as TypeScript types.
