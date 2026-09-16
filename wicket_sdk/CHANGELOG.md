# Changelog

The package's version is the SDK's, and its major is the protocol's:
`1.x` speaks protocol 1, served by the app at `/sdk/v1`. A breaking
change to the protocol is a new major and a new path.

## 1.8.0

The first release as a package, `wicket-plugin`. The SDK the app serves
is unchanged; around it:

- `wicket-plugin create <name>`: a plugin folder from the `plain` or the
  `vite` template, with its schemas, view, fixture, test, README and
  release workflow, starting at `0.1.0`.
- `wicket-plugin dev [dir]`: the shell that runs a view in the browser
  without the app, now open by default and printing the app's `--link`
  line.
- `wicket-plugin test [dir]`: the plugin's tests under the harness, with
  Playwright from the plugin's own dependencies.
- `wicket-plugin check [dir]`: what the app's inspect would say of the
  folder, with `--json`.
- `wicket-plugin/testing`: `mountPlugin`, `fixture`, `gateFrom`, as
  JavaScript with a declaration file.
- `wicket-plugin/types`: the manifest, the envelope, the messages both
  ways and `window.Wicket`, as TypeScript.
- The harness and the shell hand a view the envelope the app sends
  (`plugin`, `origin`, `revises`), not the old server's names.
- Icons in the shell and the harness come from the pinned `lucide-static`
  release, so they render outside a checkout of the app.
