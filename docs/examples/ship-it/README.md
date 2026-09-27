# Ship it?

One plugin built four ways, for the docs' [Building with a framework](../../building/frameworks.md):
a deploy waiting on a person, who sees what goes out and how the checks
went, and ships or holds it with a note.

| Folder | The view in |
|---|---|
| `vanilla/` | TypeScript, no framework |
| `react/` | React 19 |
| `vue/` | Vue 3 |
| `svelte/` | Svelte 5 |

Each is a whole plugin: the same manifest, schemas, fixtures and test, and
a view in its framework, built by Vite into `view/`. `same.test.mjs` checks
that everything but the view's code is the same in all four.

```sh
cd react
npm install
npm test                                  # build, then the test under the harness
npx pinrail-plugin dev                    # the view in a browser, on the fixtures
pinrail plugins install . --link          # in the app
```

Here the SDK comes from this repository (`file:../../../../pinrail-plugin`).
A plugin written by the SDK's `create` command takes it from the SDK's
GitHub release instead.
