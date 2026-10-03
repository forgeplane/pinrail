# Plugins

This folder holds Pinrail's official plugins. A plugin defines one kind of
review: the payload an agent sends, the decision the person gives back, and
the view the person decides in.

Two plugins are built into the app: `list` and `feedback`. The app carries
them in its binary, so they are always available. The others are optional,
and are installed one at a time, from a plugin's zip on a release or from
its folder here:

```sh
pinrail plugins install ~/Downloads/review-1.2.0.zip
pinrail plugins install ./plugins/review
```

`artifact` and `model` have a build step: run `npm ci && npm run build` in
their folder before installing it.

| Plugin | For |
|---|---|
| [`list`](list/README.md) | a list of proposed actions, each accepted or rejected (built in) |
| [`feedback`](feedback/README.md) | questions an agent wants answered before it continues, answered in one pass (built in) |
| [`review`](review/README.md) | a code review: the diff, the agent's proposed comments, and the person's verdicts and comments |
| [`email`](email/README.md) | emails an agent wants to send, to edit, send, revise or discard |
| [`artifact`](artifact/README.md) | an HTML page an agent designed, commented on element by element |
| [`calendar`](calendar/README.md) | times to arrange around what is already booked |
| [`logo`](logo/README.md) | candidate logo marks and icons, shown at every size |
| [`model`](model/README.md) | candidate 3D models, viewed on a stage, with changes requested on their parts |
| [`hello`](hello/README.md) | the smallest complete plugin, used in tests and as an example to copy |

## Writing a plugin

Plugins live in their own repositories, and this folder does not accept new
ones. To write your own, start with these guides:

- [Writing a plugin](../docs/building/writing.md): the files, the view and the
  tests, from `pinrail plugins new` to a working plugin.
- [Building with a framework](../docs/building/frameworks.md): views built
  with React, Vue, Svelte or TypeScript.
- [Settings and keys of a plugin](../docs/building/settings-and-keys.md)
- [Design and styling](../docs/building/design.md)
- [The protocol](../docs/building/protocol.md): the messages between the app
  and a plugin's view.
- [Publishing a plugin](../docs/building/publishing.md)
- [The manifest reference](https://pinrail.dev/docs/reference/manifest/), generated from the
  app's own schema.

## Layout

Every plugin here uses the same layout:

```
review/
  manifest.json           # name, version, title and the plugin's declarations
  README.md
  icon.svg                # the plugin's icon
  view/index.html         # the page the app serves, with the files it loads
  schemas/                # payload.schema.json and decision.schema.json
  templates/              # decision.md.j2, when the plugin writes its own Markdown
  samples/                # <name>.json, reviews to try the plugin with; the first is the agents' example
  fixtures/               # payloads for development and tests, and recorded decisions
  tests/                  # the plugin's Playwright tests under the SDK's harness
  src/                    # only for a plugin with a build: the sources the build turns into view/
```

When a plugin is installed, only `manifest.json`, `icon.svg`, `README.md`,
`LICENSE` and the folders `schemas/`, `view/`, `templates/` and `samples/`
are copied into the app, without hidden files.

A plugin's view always runs in a sandbox, however the plugin was
installed, and installing a plugin runs nothing. See
[What runs where](../docs/concepts/trust.md) for details.

## Tests

Each plugin's `tests/` mount its view under the harness from
[`pinrail-plugin`](../pinrail-plugin/README.md), with the payloads in its
`fixtures/`. Run them for every plugin with:

```sh
mise run test:plugins
```

While you work on a plugin, `mise run dev:plugin plugins/<name>` opens its
view in a browser, with its fixtures and without the app.

A recorded decision, `fixtures/<name>.decided.json`, holds a review's
`title`, `payload` and `decision`, and optionally its `origin` and
`agent_note`. The desktop core's tests render each one to Markdown, through
the plugin's template when it has one, and compare the result with the
`<name>.decided.md` file beside it. When a change to a template is meant to
change that output, rewrite the files and review the difference before you
commit it:

```sh
cd desktop && UPDATE_FIXTURES=1 cargo test -p pinrail-core --lib decided_fixtures
```

## Releasing

A tag named `plugin-<name>-v<version>`, such as `plugin-review-v1.2.0`, runs
[`plugin-release.yml`](../.github/workflows/plugin-release.yml). It builds
the plugin when its `package.json` has a `build` script, checks that the
manifest's version matches the tag, and attaches the bundle,
`<name>-<version>.zip`, to a GitHub release of the same tag. People can then
download the zip and install the plugin without building it:

```sh
pinrail plugins install ~/Downloads/review-1.2.0.zip
```
