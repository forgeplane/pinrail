# Screenshots

The pictures of the app in the docs and on the website, made from fixtures
and the same every time.

```sh
mise run screenshots                     # every scene, light and dark
mise run screenshots -- --only review    # scenes whose name starts with it
mise run screenshots -- --no-build       # reuse the desktop binary as it is
mise run screenshots -- --out /tmp/shots # somewhere other than the website
```

Each scene is saved as `<name>-light.png` and `<name>-dark.png` in
`website/public/screenshots`, where the docs use them as
`![alt](screenshot:<name> "caption")` and show the reader's theme. Shots the
landing pages use are copied into `website/src/assets/screenshots` as well.
`made.json` records the files a full run made. When a full run succeeds, it
removes the files an earlier run made and this one did not, such as the shots
of a renamed scene, and records its own. A file that no run made, such as a
shot taken by hand, stays. A run with `--only` or `--out`, or with a failed
scene, removes nothing.

## How a run goes

1. `app.mjs` starts a clean app: the desktop core headless on a scratch data
   directory in `e2e/.state`, and the shell from vite against it.
2. `seed.mjs` installs the official plugins the fixtures use from a checkout
   of [forgeplane/pinrail-plugins](https://github.com/forgeplane/pinrail-plugins)
   beside this repository, `../pinrail-plugins`, or from the folder that
   `PINRAIL_PLUGINS_DIR` names. Build `artifact` and `model` there first
   (`npm ci && npm run build` in each). It then creates each fixture in
   `fixtures/` through the API, with its decision, discard or withdrawal.
3. The core is stopped and its database pinned: every time counts back from
   a fixed moment, ids, the person deciding and the plugins' sources are
   fixed values, and the core starts again.
4. `run.mjs` opens each scene in `scenes.mjs` once per theme, with the
   browser's clock frozen at the same moment, and shoots it. Paths and notes
   only a browser shows are made to read as the app shows them.

## Fixtures

One JSON file per review, created in file-name order:

```json
{
  "plugin": "list",
  "title": "Dependency upgrades for web-app",
  "origin": { "repo": "northwind/web-app", "workflow": "deps" },
  "requested_by": "renovate-agent",
  "age": "12m",
  "payload": { "groups": [] }
}
```

`age` is how long before the fixed moment the review was made: `"40s"`,
`"12m"`, `"3h"`, `"2d"`. A review that has ended adds `decision` (with an
optional `note` to the agent), `discard` or `withdraw` (the reason), and
`decided`, when it ended. `revises` names the fixture a round answers. The
payload and decision must pass the plugin's schemas, as any agent's would.

## Scenes

A scene is `{ name, run }`. `run` gets the page, the app, the review ids by
fixture name, and `shot(name, target?, options?)`:

```js
{
  name: "list",
  async run({ page, app, reviews, shot }) {
    const f = await openReview(page, app, reviews["13-list-deps"]);
    await f.locator('[data-id="1"] button', { hasText: "Accept" }).click();
    await shot("list");                                                 // the whole app
    await shot("list-view", page.locator("#plugin-frame"), { site: true }); // the view, for the website too
  },
}
```

Show a decision under way: verdicts given, a note or a comment half
written. Wait for what the shot needs to have drawn, not for a time alone,
and set a scroll position rather than leaving it to where the clicks left
it. A scene whose picture changes between two runs has a wait missing.
