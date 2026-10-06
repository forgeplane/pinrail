# The dev shell

The plugin SDK's dev shell shows the view in a browser, inside a stand-in
for the app, and reloads it whenever a file of the plugin changes. Work on
the view there with the person. It is quicker than sending reviews, and
the person can comment on any part of the view for you to revise.

## Start it

Serve the plugin with the SDK of the same version as this Pinrail, and
give the person the address it prints, `http://127.0.0.1:4790` unless you
pass `--port`:

```sh
npx pinrail-sdk@{{sdk_version}} dev <path> --no-open
```

It needs Node, and nothing installed in the folder. Keep it running while
you work: it reloads the view on every change.

## What it shows

- The bar picks the review the view opens with, from `samples/` and
  `fixtures/`, a decided one as the previous round, and read-only. It also
  switches the theme.
- *JSON* shows the payload, the payload schema and the decision schema in
  place of the view.
- The side panel plays the app's part: the hand-over, the settings and
  keys the manifest declares, and what the view sent, such as its draft
  and its decision, checked against the decision schema.

## Comments

The person turns on *Select*, or presses I, clicks a part of the view, and
writes what should change there. Each comment keeps the element it is
about. *Copy comments* puts all of them on the clipboard as Markdown, and
the person pastes them to you.

Make the changes, and the view reloads by itself. The person then clears
the comments in the shell, so the next round starts empty, and comments
again. Repeat until the person is satisfied with the view, then try the plugin
in the app.
