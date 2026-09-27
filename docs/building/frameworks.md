---
title: Building with a framework
description: "A plugin's view in React, Vue, Svelte or plain TypeScript: any toolchain works, as long as its build ends in an HTML page in the plugin folder. A tutorial, in each of them."
---

:::note[Coming with the SDK package]
This page describes the SDK package, which comes in a later release. Until then, a plugin starts from `pinrail plugins new`: a view with no build. See [Writing a plugin](/docs/building/writing/).
:::

A view is an HTML page the app loads. It does not matter how that page is made: by hand, or by React, Vue, Svelte or any other framework and its build tool. As long as the build writes an HTML page and its scripts into the plugin folder, the app serves it like any other.

This page builds one plugin, **Ship it?**, in four ways. An agent is about to deploy; the person sees what goes out and how the checks went, and ships or holds it with a note.

![Ship it?, built with React: a deploy of payments-api v2.4.1 with a failing canary check, Ship chosen and a note to the agent written.](screenshot:ship-it "Ship it?: the changes going out, the checks, and the choice, with the hand-over button saying what it will do.")

:::tip[Simple plugins stay simple]
A plugin with a view of a screen or two needs no framework and no build: an HTML page with an inline script is enough, and there is nothing to install, build or keep up to date. The [List](/docs/plugins/list/) and [Logo](/docs/plugins/logo/) plugins' views are plain HTML. Reach for a framework when the view has enough state and pieces to need one.
:::

## What the app needs from a build

- **An HTML page in the folder.** The manifest's `entry` names the built page, such as `view/index.html`, and the page's scripts and styles sit beside it.
- **A `build` command in the manifest**, such as `npm ci && npm run build`. Installing from a folder or a repository runs it, after showing it to the person. A plugin linked for development (`--link`) is served as it is, so build it yourself first.
- **Relative paths.** The app serves the plugin under a path of its own, so the build must refer to its files relatively: with Vite, `base: "./"`.
- **The SDK from the app.** Load `/sdk/v1/pinrail-plugin.js` and its stylesheet with tags in the page; don't bundle them. The package gives your code the types: `@forgeplane/pinrail-plugin/types`.
- **Everything else bundled.** The frame loads nothing from the network, so the framework itself, fonts and images go into the build.

The plugin's manifest, and the schemas every framework's version shares:

![The Ship it? plugin's contract](contract:ship-it/react)

## 1. Create the folder

Every version is a Vite project whose build writes `view/`. To start one of your own, `create` writes a working plugin in each of these frameworks, a yes-or-no question to build on, with a `sample.json` to send and an `AGENTS.md` that explains the plugin to a coding agent:

```sh
npx @forgeplane/pinrail-plugin create push_check --template react    # or vite (TypeScript), vue, svelte
```

This page builds Ship it? instead. Choose a framework, and every example on this page follows it:

![TypeScript](example:ship-it/vanilla/package.json) ![TypeScript](example:ship-it/vanilla/vite.config.ts)
![React](example:ship-it/react/package.json) ![React](example:ship-it/react/vite.config.ts)
![Vue](example:ship-it/vue/package.json) ![Vue](example:ship-it/vue/vite.config.ts)
![Svelte](example:ship-it/svelte/package.json) ![Svelte](example:ship-it/svelte/vite.config.ts)

In your own plugin the SDK comes from npm, as `"@forgeplane/pinrail-plugin": "^1"`; the examples take it from the Pinrail repository.

The manifest is the same for every framework. Its `entry` is the built page, and `build` is the command an install runs:

![Manifest](example:ship-it/react/manifest.json)

## 2. The page

The page loads the SDK and its stylesheet from the app, and your code from the build. It is the same in every framework but for the script it loads:

![TypeScript](example:ship-it/vanilla/src/index.html)
![React](example:ship-it/react/src/index.html)
![Vue](example:ship-it/vue/src/index.html)
![Svelte](example:ship-it/svelte/src/index.html)

## 3. The view

The view connects to the app once, with `Pinrail.connect`, and draws the review from what `onInit` hands it. It keeps the person's choice as a draft, tells the hand-over button what it will do with `status`, and submits when the app sends `collect`:

![TypeScript](example:ship-it/vanilla/src/main.ts)
![React](example:ship-it/react/src/main.tsx) ![React](example:ship-it/react/src/App.tsx)
![Vue](example:ship-it/vue/src/main.ts) ![Vue](example:ship-it/vue/src/App.vue)
![Svelte](example:ship-it/svelte/src/main.ts) ![Svelte](example:ship-it/svelte/src/App.svelte)

## 4. Try it and test it

```sh
npm install
npm run watch              # rebuilds view/ as you save…
npx pinrail-plugin dev     # …and shows it in a browser, on the fixtures
npx pinrail-plugin check   # what the app would say of the folder
npm test                   # build, then the tests under the harness
```

:::tip[pinrail-plugin dev: the app, without the app]
`pinrail-plugin dev` opens the view in a browser inside a stand-in for the app, and reloads it when the build changes. The bar at the top picks a fixture, a decided one as the previous round, or read-only, and plays the app's side: *Collect* is the hand-over button, *Theme* switches light and dark. On the right: the settings and keys the manifest declares, what the view last sent as its status, draft and decision, every message in both directions, and violations or a decision to send back.

![pinrail-plugin dev with Ship it?: the view on the left with Ship chosen, and on the right the shortcuts s and h, the status Ship v2.4.1, the draft, and the draft and status messages the view sent.](screenshot:dev-shell "pinrail-plugin dev: the view, what it sent, and the app's side of the conversation to play.")
:::

:::note[pinrail-plugin check: what the app would say]
`pinrail-plugin check` reads the folder the way the app does when you install it, without the app running. It reports **problems**, which keep the plugin from installing: a malformed manifest, schemas that are not JSON Schema, an entry that is missing (a folder with a build is given until the build has run). And **warnings**, which cost a feature: settings or shortcuts that break their rules, an example that does not pass its own schema, a template that cannot be read. `--json` prints the same for a script or CI.
:::

A test mounts the built view alone and drives it the way a person would. Because it looks only at what the person sees (text, roles and labels), the same test passes for every framework:

![Test](example:ship-it/react/tests/ship_it.spec.ts)

## 5. Install it

```sh
npm run build
pinrail plugins install . --link
```

A link serves the folder as it is, so rebuild as you change it. Installing without `--link` copies the folder and runs the manifest's `build` in the copy, the way someone else installs your plugin from its repository.

## Things to know

- **A forwarded key has no element as its target.** The app forwards a manifest shortcut pressed outside the frame as a `keydown` on your document, so check `event.target instanceof Element` before calling `closest` on it.
- **Hand the SDK plain data.** Vue's refs and Svelte's `$state` are proxies, which a message to the app cannot carry. Pass a copy to `plugin.draft` and `plugin.submit`: `{ ...draft.value }` in Vue, `$state.snapshot(draft)` in Svelte.
- **Connect once.** Call `Pinrail.connect` when the view mounts, not on every render. Its callbacks are made once, so read the latest state from somewhere they can reach, such as a React ref.
- **Leave the hand-over to the app.** Draw no submit button; the app sends `collect` from its own. See [Design and styling](/docs/building/design/#the-design-language).

The four versions are in the repository under [`docs/examples/ship-it`](https://github.com/forgeplane/pinrail/tree/main/docs/examples/ship-it), each a whole plugin with its tests.
