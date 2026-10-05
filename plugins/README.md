# Plugins

This folder holds Pinrail's official plugins that the app carries, and the
plugins its tests use. The app carries the official plugins as a catalog:
none is installed until the person chooses it, with
`pinrail plugins install <name>`. The official
plugins that are not here yet are in
[forgeplane/pinrail-plugins](https://github.com/forgeplane/pinrail-plugins).

| Plugin | What it is for |
|---|---|
| [`list`](list/README.md) | Official: a list of proposed actions, each accepted or rejected. |
| [`feedback`](feedback/README.md) | Official: questions an agent wants answered before it continues. |
| [`hello`](hello/README.md) | The smallest complete plugin, used in tests and as an example to copy. |
| [`sampler`](sampler/README.md) | A test fixture: a plugin that takes files and declares a verdict. |

The core embeds the official plugins when it is built; the list is
`CATALOG` in `desktop/core/build.rs`. An installed copy is offered an update
when a newer app carries a higher version, so a change to an official plugin
between two releases of the app also raises the version in its manifest.

## Tests

Each plugin's `tests/` mount its view under the harness from
[`pinrail-plugin`](../pinrail-plugin/README.md), with the payloads in its
`fixtures/`. Run them with:

```sh
mise run test:plugins
```

While you work on a plugin, `mise run dev:plugin plugins/<name>` opens its
view in a browser, with its fixtures and without the app.

`pinrail plugins check plugins/<name>` says what the app would make of a
plugin, and checks its recorded decisions, `fixtures/<name>.decided.json`,
against the Markdown beside them. `--update-fixtures` rewrites that
Markdown; review the difference before you commit it.
