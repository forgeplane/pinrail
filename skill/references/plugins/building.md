# Building a plugin

Design the decision first: write `schemas/decision.schema.json` around the
exact action you will take on the answer, such as approve, send back with
changes, or stop, and what an incomplete answer means. Then build the
controls that produce it. A view that shows the right material but returns
another kind of answer does not do the job.

The app lets the person add a note to the agent with any decision, so do
not add a comment field for the whole review. Add a note field only to a
part that is answered on its own, such as an item or a passage.

```sh
pinrail plugins new <name> --dir <path>   # scaffold it; no build, nothing installed
# make it yours: the decision and payload schemas, the view, the samples
pinrail plugins check <path>              # what the app would refuse, and why
pinrail plugins install <path> --link     # the app follows the folder as it changes
pinrail submit <name> --sample            # a real review of its sample, in the person's inbox
pinrail open <id> --browser               # its preview: the view in a browser, prints the address
pinrail withdraw <id>                     # when you are done with the sample
```

- `--link` registers the folder with the app, which serves it from there
  and lists it in the person's plugins; `pinrail plugins remove <name>`
  undoes it. Linking a plugin the person's task needs is part of that
  task.
- A manifest needs `name` and `version`. Always give `use_when` too, the
  moment an agent should ask with the plugin, specific: agents choose by
  it.
- Each file has a fixed place, which the manifest does not name:
  `schemas/payload.schema.json` and `schemas/decision.schema.json`
  (required), `view/index.html` (required, with its scripts and assets in
  `view/`), `templates/decision.md.j2`, `icon.svg`, `samples/<name>.json`,
  `README.md` and `LICENSE`. An install copies these and nothing else.
  The SDK's types are in `pinrail-plugin.d.ts`.
- A sample, `samples/<name>.json`, is a whole review (`title`, `payload`
  and any `attachments`, relative to `samples/`) for people to see the
  plugin. `--sample <name>` sends one; `--sample` alone sends the first
  by name. The first sample's payload is also the example agents get
  from `pinrail plugins describe`, so keep it small. A change to the
  payload's shape touches the payload schema, the view and the samples.
- An opened pending review moves to a new release that takes its
  payload; an ended one keeps its release. A move can change the
  decision schema: on a decision you did not expect, describe the
  plugin again. A release whose schemas refuse what the previous one
  took needs a new major version (`2.0.0`, or `0.4.0` after `0.3.x`).
  `pinrail plugins check <dir> --since <previous release dir>` lists such
  breaks.
- `summary` in the manifest declares what the app counts for the inbox
  and history: arrays of the payload (`request`) and of the decision
  (`outcome`), by a field such as `severity` or `action`, each value with
  a label and a tone. The format is in
  [the manifest](building/manifest.md).
- The schemas are JSON Schema 2020-12. Read them, not prose about them:
  an installed plugin's are in `pinrail plugins describe <plugin>`.
- The preview's hand-over checks the decision against the decision schema
  and decides nothing. A change to the view shows the next time the
  preview is opened; a change to the manifest or a schema of a linked
  plugin applies within a second.
- To change an installed plugin, link your copy under the same name:
  `pinrail plugins install <dir> --link`. It takes the installed plugin's
  place, and its reviews render with your folder.
- The view runs in a sandboxed frame with an opaque origin, so a
  browser tool that reads or clicks through the page's DOM or
  accessibility tree sees the frame as empty, even when the view is
  drawn. To check a view, take a screenshot, or drive the preview with
  Playwright and reach inside with `page.frameLocator("iframe")`. An
  empty frame in your tool does not mean the view is broken.

## More

- [The manifest](building/manifest.md): The JSON Schema every plugin's manifest is checked against.
- [The view](building/view.md): The SDK's contract: init, hand-over, violations, drafts, read-only.
- [Design](building/design.md): Look like the app: the stylesheet's tokens and classes, themes, icons.
- [Settings and keys](building/settings.md): Options in Settings › Plugins, and keyboard shortcuts the app lists and forwards.
- [Frameworks](building/frameworks.md): Views built with React, Vue, Svelte or TypeScript, created with the plugin SDK from a checkout of the Pinrail repository.
