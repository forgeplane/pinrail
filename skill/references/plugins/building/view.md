# The view

The view is a web page in `view/index.html`. It loads the SDK from the
app, and talks to the app only through it:

```html
<link rel="stylesheet" href="/sdk/v1/pinrail-plugin.css">
<script src="/sdk/v1/pinrail-plugin.js"></script>
<script src="view.js"></script>
```

## The contract

```js
const plugin = Pinrail.connect({
  onInit({ review, readonly, draft, settings }) {}, // review.payload is what the agent sent
  onCollect() { return decision; },                 // the person pressed the hand-over
});
plugin.draft(value);                  // keeps what the person entered across reloads
plugin.handOverLabel("Hand over: yes"); // the words on the hand-over button
```

- `onInit` draws the review. It runs again, read-only, when the review
  ends while the view is open, for example when the agent withdraws it,
  so draw the view from scratch each time.
- The app draws the hand-over button, and the view never draws its own.
  `onCollect` returns the decision, shaped by the decision schema, or a
  promise of it. It returns nothing while the view needs more from the
  person, and the next press asks again.
- The app checks the decision against the schema. It lists a refused
  decision's violations under the view, and closes the view once a
  decision is accepted. A view that marks the field at fault itself can
  use `onViolations(errors)`, and one that redraws itself `onSubmitted()`.
- `plugin.readonly` is true for a review that is no longer pending. Show
  `review.decision.data`, and offer no editing.
- Every call and its arguments are in `pinrail-plugin.d.ts`.

## What the view can use

- The frame loads nothing from outside the plugin folder, so the payload
  carries everything the view shows.
- Files: declare `attachments` in the manifest, name each one in the
  payload as `{ "$attachment": "<name>" }`, and show it with
  `await plugin.attachmentUrl(name)`.
- Links: `plugin.open(url)`. The app asks the person before it opens a
  link, unless they allowed that site for the plugin.
- Rendering: `Pinrail.escape(text)`, `Pinrail.icon(name)`, and
  `Pinrail.markdown(text)` once the page also loads
  `<script src="/sdk/v1/markdown.js"></script>` after the SDK.
- Layout: the view fills the review panel's height and scrolls inside its
  frame. `Pinrail.layout()` keeps a header in place over a body that
  scrolls.

## Checking the view

The view runs in a sandboxed frame with an opaque origin, so a browser
tool that reads the page's DOM or accessibility tree sees the frame as
empty, even when the view is drawn. To check the view, take a screenshot,
or drive the preview with Playwright and reach inside with
`page.frameLocator("iframe")`. An empty frame in your tool does not mean
that the view is broken.
