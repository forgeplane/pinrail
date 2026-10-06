---
title: Building with a framework
description: "A plugin's view in React, Vue, Svelte or plain TypeScript: any toolchain works, as long as its build ends in an HTML page in the plugin folder. A tutorial, in each of them."
---

A view is an HTML page the app loads. It does not matter how that page is made: by hand, or by React, Vue, Svelte or any other framework and its build tool. As long as the build writes an HTML page and its scripts into the plugin folder, the app serves it like any other.

This page builds one plugin, **Ship it?**, in four ways. An agent is about to deploy; the person sees what goes out and how the checks went, and ships or holds it with a note.

![Ship it?, built with React: a deploy of payments-api v2.4.1 with a failing canary check, Ship chosen and a note to the agent written.](screenshot:ship-it "Ship it?: the changes going out, the checks, and the choice, with the hand-over button saying what it will do.")

:::tip[Simple plugins stay simple]
A plugin with a view of a screen or two needs no framework and no build: an HTML page with an inline script is enough, and there is nothing to install, build or keep up to date. The [List](/docs/plugins/list/) and [Image review](/docs/plugins/image/) plugins' views are plain HTML. Reach for a framework when the view has enough state and pieces to need one.
:::

## What the app needs from a build

- **An HTML page at `view/index.html`.** The build writes the page there, and the page's scripts and styles beside it in `view/`.
- **A build you run before installing.** Pinrail installs a plugin as it is and runs nothing, so run the build, such as `npm run build`, before you install or link the folder, and before you zip it for a release. The release workflow of [Publishing a plugin](/docs/building/publishing/) runs it for you.
- **Relative paths.** The app serves the plugin under a path of its own, so the build must refer to its files relatively: with Vite, `base: "./"`.
- **The SDK from the app.** Load `/sdk/v1/pinrail-plugin.js`, its stylesheet and, to render Markdown, `/sdk/v1/markdown.js` with tags in the page; don't bundle them. The package gives your code the types: `pinrail-sdk/types`.
- **Everything else bundled.** The frame loads nothing from the network, so the framework itself, fonts and images go into the build.

The plugin's manifest, and the schemas every framework's version shares:

![The Ship it? plugin's contract](contract:ship-it/react)

## 1. Create the folder

Every version is a Vite project whose build writes `view/`. To start one of your own, `pinrail plugins new` writes a working plugin in React or TypeScript, a yes-or-no question to build on, with a sample to send. `--playwright` adds a first test. For Vue or Svelte, start from the TypeScript template and follow the Vue or Svelte version of this page:

```sh
pinrail plugins new push_check --template react    # or --template vite, in TypeScript
```

This page builds Ship it? instead. Choose a framework, and every example on this page follows it:

![TypeScript](example:ship-it/vanilla/package.json) ![TypeScript](example:ship-it/vanilla/vite.config.ts)
![React](example:ship-it/react/package.json) ![React](example:ship-it/react/vite.config.ts)
![Vue](example:ship-it/vue/package.json) ![Vue](example:ship-it/vue/vite.config.ts)
![Svelte](example:ship-it/svelte/package.json) ![Svelte](example:ship-it/svelte/vite.config.ts)

A plugin written by `pinrail plugins new` takes the SDK's types from `pinrail-plugin.d.ts` in its folder, and needs the SDK package only for its tests. The examples on this page take the package from the Pinrail repository, because the SDK is not published to npm.

The manifest is the same for every framework. Its `build` is the command an install runs:

![Manifest](example:ship-it/react/manifest.json)

## 2. The page

The page loads the SDK and its stylesheet from the app, and your code from the build. It is the same in every framework but for the script it loads:

![TypeScript](example:ship-it/vanilla/src/index.html)
![React](example:ship-it/react/src/index.html)
![Vue](example:ship-it/vue/src/index.html)
![Svelte](example:ship-it/svelte/src/index.html)

## 3. The view

The view connects to the app once, with `Pinrail.connect`, and draws the review from what `onInit` hands it. It keeps the person's choice as a draft, tells the hand-over button what it will do with `handOverLabel`, and returns its decision from `onCollect` when the person hands over. It needs no other handler: the app lists a refused decision's violations under the view, and closes the view once a decision is accepted. `onViolations` and `onSubmitted` remain for a view that marks the field at fault or redraws itself:

![TypeScript](example:ship-it/vanilla/src/main.ts)
![React](example:ship-it/react/src/main.tsx) ![React](example:ship-it/react/src/App.tsx)
![Vue](example:ship-it/vue/src/main.ts) ![Vue](example:ship-it/vue/src/App.vue)
![Svelte](example:ship-it/svelte/src/main.ts) ![Svelte](example:ship-it/svelte/src/App.svelte)

## 4. Try it and test it

```sh
npm install
npm run watch              # rebuilds view/ as you save…
npx pinrail-sdk dev     # …and shows it in a browser, on the fixtures
npx pinrail-sdk check   # what the app would say of the folder
npm test                   # build, then the tests under the harness
```

:::tip[pinrail-sdk dev: a stand-in for the app]
`pinrail-sdk dev` opens the view in a browser inside a stand-in for the app, and reloads it when the build changes. The bar at the top picks a fixture, a decided one as the previous round, or read-only, and plays the app's side: *Collect* is the hand-over button, *Theme* switches light and dark. On the right: the settings and keys the manifest declares, what the view last sent as its status, draft and decision, every message in both directions, and violations or a decision to send back. *Select* lets you comment on any part of the view and copy the comments to an agent, as [Writing a plugin](/docs/building/writing/#work-on-the-view-in-the-dev-shell) describes.

![pinrail-sdk dev with Ship it?: the view on the left with Ship chosen, and on the right the shortcuts s and h, the status Ship v2.4.1, the draft, and the draft and status messages the view sent.](screenshot:dev-shell "pinrail-sdk dev: the view, what it sent, and the app's side of the conversation to play.")
:::

:::note[pinrail-sdk check: what the app would say]
`pinrail-sdk check` runs `pinrail plugins check`, which reads the folder the way the app does when you install it, without the app running, and reports two kinds of result. **Problems** prevent installation: a malformed manifest, a schema that is not valid JSON Schema, or a missing `view/index.html`. Run the build first, so that `view/index.html` is there. **Warnings** disable one feature and leave the plugin working: settings or shortcuts that break their rules, a sample that does not pass the payload schema, or a template that cannot be read. `--json` prints the result for a script or CI.

The command also checks the plugin's recorded decisions, the `fixtures/<name>.decided.json` files that tests use to show a decided review. Each one's payload and decision must pass the plugin's schemas. When a `<name>.decided.md` file is beside a recorded decision, the Markdown that the app renders for an agent from it must equal that file, so a change to the plugin's template or schemas cannot change what agents read without you noticing. `--update-fixtures` writes these files from what the app renders. Review the difference before you commit it.
:::

A test mounts the built view alone and drives it the way a person would. Because it looks only at what the person sees (text, roles and labels), the same test passes for every framework:

![Test](example:ship-it/react/tests/ship_it.spec.ts)

## 5. Install it

```sh
npm run build
pinrail plugins install . --link
```

A link follows the folder as it is, so rebuild as you change it, or keep `npm run watch` running. Installing without `--link` copies the built folder into the app, without its sources.

## Things to know

- **A forwarded key has no element as its target.** The app forwards a manifest shortcut pressed outside the frame as a `keydown` on your document, so check `event.target instanceof Element` before calling `closest` on it.
- **Hand the SDK your state as it is.** Vue's refs and Svelte's `$state` are proxies, which a message to the app cannot carry, so the SDK sends a plain copy of a draft or a decision.
- **Connect once, where the page starts.** Call `Pinrail.connect` in the entry module, not in a component: a framework may mount a component more than once, and a second call throws. Render the component when `onInit` arrives. The connection's callbacks are made once, so they call into the component on screen, which reads its latest state from somewhere they can reach, such as a React ref.
- **Leave the hand-over to the app.** Draw no submit button; the app asks for the decision from its own. See [Design and styling](/docs/building/design/#the-design-language).

The four versions are in the repository under [`docs/examples/ship-it`](https://github.com/forgeplane/pinrail/tree/main/docs/examples/ship-it), each a whole plugin with its tests.
