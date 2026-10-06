# Building a plugin

A plugin is a folder: what the agent sends, what the person sees, and the
decision the agent gets back. This guide goes from a new folder to a
review in the person's inbox.

## 1. Create the plugin

```sh
pinrail plugins new <name> --dir <path>
```

This writes a working plugin that asks one yes-or-no question. It needs
no build and installs nothing. For a view in React, Vue, Svelte or
TypeScript, see [Frameworks](building/frameworks.md).

## 2. The files

Each file has a fixed place, which the manifest does not name. An install
copies these files and nothing else:

- `manifest.json`: `name` and `version` are required. Always give
  `use_when` as well: the moment an agent should ask with the plugin,
  stated precisely, because agents choose by it. `summary` declares what
  the inbox counts, such as items by severity. [The manifest](building/manifest.md)
  lists every key.
- `schemas/decision.schema.json`: what the view hands back.
- `schemas/payload.schema.json`: what the agent sends.
- `view/index.html`, with its scripts, styles and icons in `view/`.
- `samples/<name>.json`: whole reviews that show the plugin at work.
- `icon.svg`, `README.md`, and optionally `LICENSE` and
  `templates/decision.md.j2`.

`pinrail-plugin.d.ts`, the SDK's types, is for your editor while you work,
and is not installed.

## 3. Design the decision

Start with `schemas/decision.schema.json`. Shape it around the exact
action you will take on the answer, such as approve, send back with
changes, or stop, and say what an incomplete answer means. A view that
shows the right material but returns another kind of answer does not do
the job.

The app lets the person add a note to the agent with any decision, so do
not add a comment field for the whole review. Add a note field only to a
part that is answered on its own, such as an item or a passage.

## 4. The payload and the samples

Then write `schemas/payload.schema.json`. The view can load nothing from
outside the folder, so the payload carries everything it shows. Both
schemas are JSON Schema 2020-12.

A sample is a whole review: a `title`, a `payload`, and any
`attachments`, relative to `samples/`. The first sample's payload is also
the example that `pinrail plugins describe` gives agents, so keep it
small. A change to the payload's shape touches the payload schema, the
view and the samples together.

## 5. The view

Rewrite `view/view.js` around your decision.
[The view](building/view.md) gives its contract with the app,
[design](building/design.md) the app's styles, and
[settings and keys](building/settings.md) the options and keyboard
shortcuts a plugin can declare.

## 6. Work on the view with the person

Run the SDK's dev shell, which shows the view to the person in a browser
and reloads it as you change it. The person comments on any part of the
view and pastes the comments to you. Revise the view, and repeat until
the person is satisfied. [The dev shell](building/dev-shell.md) explains
how to start it, and [the view](building/view.md#checking-the-view) how
to look at the view yourself.

## 7. Try it in the app

```sh
pinrail plugins check <path>              # what the app would refuse, and why
pinrail plugins install <path> --link     # the app serves the folder as it changes
pinrail submit <name> --sample            # a real review of the first sample
pinrail withdraw <id>                     # when you are done with the sample
```

A linked plugin is listed in the person's plugins until
`pinrail plugins remove <name>`. Linking a plugin that the person's task
needs is part of that task. A change to the manifest or a schema applies
within a second.

## 8. Change it later

- A release whose schemas refuse what the previous one accepted needs a
  new major version, such as `2.0.0`, or `0.4.0` after `0.3.x`.
  `pinrail plugins check <dir> --since <previous release dir>` lists such
  breaks.
- An opened pending review moves to a new release that accepts its
  payload, and an ended one keeps its release. On a decision you did not
  expect, describe the plugin again.
- To change an installed plugin, link your copy under the same name. It
  takes the installed plugin's place.

## More

- [The view](building/view.md): The SDK's contract: init, hand-over, violations, drafts, read-only.
- [Design](building/design.md): Look like the app: the stylesheet's tokens and classes, themes, icons.
- [The dev shell](building/dev-shell.md): The view in a browser, reloading as you change it, with comments from the person to revise it by.
- [Settings and keys](building/settings.md): Options in Settings › Plugins, and keyboard shortcuts the app lists and forwards.
- [The manifest](building/manifest.md): The JSON Schema every plugin's manifest is checked against.
- [Frameworks](building/frameworks.md): Views built with React, Vue, Svelte or TypeScript, created with the plugin SDK from a checkout of the Pinrail repository.
