# The view

The view loads the SDK from the app and talks to it only through it:

```html
<link rel="stylesheet" href="/sdk/v1/pinrail-plugin.css">
<script src="/sdk/v1/pinrail-plugin.js"></script>
<script src="view.js"></script>
```

The calls, as a sketch: `decision` is what the person's controls built,
shaped by your decision schema, and `value` whatever state you keep.

```js
const plugin = Pinrail.connect({
  onInit({ review, readonly, draft, settings }) {}, // review.payload is what the agent sent
  onCollect() { return decision; },               // the app's hand-over button, or ⌘/Ctrl+Enter
});
plugin.draft(value);                              // keeps what the person entered across reloads
plugin.handOverLabel("Hand over: yes");       // the hand-over button's words
```

- A view needs `onInit` and `onCollect`. The app lists a refused
  decision's violations under the view and closes the view once a
  decision is accepted. `onViolations(errors)` and `onSubmitted()` remain
  for a view that marks the field at fault or redraws itself.
- `onInit` runs again, read-only, when the review ends while the view is open, for example when the agent withdraws it. Draw the view from scratch each time.
- The app draws the hand-over button; the view never draws its own.
  `onCollect` returns the decision, or a promise of it. It returns
  nothing when the view needs more from the person first, and the next
  press asks again.
- `plugin.readonly` is true for a review that is no longer pending: show
  `review.decision.data` and offer no editing.
- The frame loads nothing from outside the plugin folder: the payload
  carries everything the view shows.
- Files: declare `attachments` in the manifest, name them in the payload
  as `{ "$attachment": "<name>" }`, and show them with
  `await plugin.attachmentUrl(name)`.
- Links: `plugin.open(url)`. The app asks the person before it opens a
  link, unless they allowed that site for the plugin.
- Rendering: `Pinrail.escape(text)`, `Pinrail.icon(name)`, and
  `Pinrail.markdown(text)` once the view also loads
  `<script src="/sdk/v1/markdown.js"></script>` after the SDK.
- The view fills the review panel's height and scrolls inside its frame;
  `Pinrail.layout()` keeps a header in place over a body that scrolls.
- Every call with its arguments is in `pinrail-plugin.d.ts`.
