---
title: Design and styling
description: "How a view looks like part of Pinrail: the design language, the SDK stylesheet's tokens and classes, themes, type, icons, and bringing your own fonts, styles and scripts."
---

A view sits inside the app, between the review's header and the hand-over button, so the person reads it as part of Pinrail. This page is how to make it look that way with little effort: the conventions the built-in plugins follow, and what the SDK stylesheet gives you.

:::tip[All of this is optional]
None of this is required. The stylesheet is a set of defaults: every rule in it is in a cascade layer, so any rule of your own wins, whatever its specificity. Use its tokens and ignore its classes, restyle everything, or bring a design system of your own, as long as it ships inside the plugin folder (see [Your own fonts, styles and scripts](#your-own-fonts-styles-and-scripts)).
:::

## The design language

What makes a view feel native, whatever it draws:

- **The app owns the hand-over.** Do not draw a submit button, and do not repeat the review's title. The app shows the title above the view and the hand-over button below it, in the same place for every plugin. A header of your own, such as the one `Pinrail.layout()` draws, can name what the view lists and show counts. Tell the hand-over button what it will do with `plugin.handOverLabel`, such as *Hand over 3 of 5*.
- **Dense and quiet.** Text at 13px, one accent colour, lines rather than boxes. What matters is the work being reviewed, not the frame around it.
- **Keys for the common path.** Most plugins move with <kbd>j</kbd> and <kbd>k</kbd> and give a verdict with one key each. Declare them in the manifest so the app lists them in its keyboard help (see [Settings and keys](/docs/building/settings-and-keys/)).
- **Read-only is a full view.** A decided review is read months later: render what was there and what was decided, without the controls.
- **A brand's colours stay in the content.** A plugin that shows something in its own palette, as the Logo plugin shows marks in the brand's colours, paints that inside its content; the chrome around it keeps the app's tokens.

## The anatomy of a view

A review screen has five parts. The app draws two of them around the view, and `Pinrail.layout()` draws the three inside it:

<figure class="pr-anatomy not-content" aria-label="The parts of a review screen">
  <div class="pr-anatomy-app"><span class="pr-anatomy-n">1</span><b>Sentry triage</b><span>acme-api · 5 minutes ago</span></div>
  <div class="pr-anatomy-view">
    <div class="pr-anatomy-header"><span class="pr-anatomy-n">2</span><b>4 items</b><span class="pr-anatomy-chip">1 accepted</span><span class="pr-anatomy-chip">3 undecided</span><span class="pr-anatomy-push">Accept all</span></div>
    <div class="pr-anatomy-middle">
      <div class="pr-anatomy-side">Your own sidebar, if the view needs one</div>
      <div class="pr-anatomy-body"><span class="pr-anatomy-n">3</span><div><b>The view's content</b><p>It scrolls; the parts above and below it stay in place.</p></div></div>
    </div>
    <div class="pr-anatomy-confirm"><span class="pr-anatomy-n">4</span>3 left undecided. Hand over again to confirm.<span class="pr-anatomy-push">Keep deciding</span></div>
  </div>
  <div class="pr-anatomy-app pr-anatomy-composer"><span class="pr-anatomy-n">5</span><span>Add a note for the agent…</span><span class="pr-anatomy-push pr-anatomy-handover">Hand over with 3 undecided</span></div>
  <figcaption>The app draws 1 and 5. <code>Pinrail.layout()</code> draws 2, 3 and 4 inside the view's frame.</figcaption>
</figure>

| | Part | Drawn by | What goes there |
|---|---|---|---|
| 1 | Review bar | The app | The review's title and where it comes from. Do not repeat them in the view. |
| 2 | Header | `Pinrail.layout()`, as `.pinrail-header` | What the view lists, its counts, and controls that act on all of it, such as *Accept all*. It stays in place while the body scrolls. |
| 3 | Body | `Pinrail.layout()`, as `.pinrail-scroll` and `.pinrail-content` | The view's content, which scrolls. A sidebar of the view's own, such as the item list in the List and Code review plugins, goes beside it. |
| 4 | Confirmation bar | `view.confirmation()`, as `.pinrail-confirmation` | What the person must confirm before the next hand-over, such as items left undecided. It is hidden until the view asks for it. |
| 5 | Composer | The app | The note to the agent and the hand-over button. The view sets the button's label with `plugin.handOverLabel`. |

These positions are a convention, not a rule. The built-in plugins follow them so that every review reads the same way, and `Pinrail.layout()` builds them for you. A view that needs another arrangement can skip `Pinrail.layout()` and lay out its frame as it likes, or use the layout and restyle any part of it. Only parts 1 and 5 are fixed, because the app draws them outside the view.

## The stylesheet

```html
<link rel="stylesheet" href="/sdk/v1/pinrail-plugin.css">
```

It sets the base (type, links, focus rings, code, tables, narrow scrollbars), the colour tokens below in both themes, and a few classes for the shapes most views need. It also styles what `Pinrail.markdown` renders: headings, lists, quotes, tables and code.

Every class it defines starts with `pinrail-`, every token with `--pinrail-`, and every rule is in the cascade layer `pinrail`. A component library's own names, such as `.btn` or `--border`, therefore keep their meaning beside it. A view that brings such a library and wants only the palette links the tokens alone:

```html
<link rel="stylesheet" href="/sdk/v1/tokens.css">
```

### Colour tokens

Style with the tokens rather than with colours, and your view follows the app in both themes with no palette of its own to keep in step.

![The SDK stylesheet's colour tokens](tokens:)

```css
.card {
  background: var(--pinrail-bg-raised);
  border: 1px solid var(--pinrail-border);
  color: var(--pinrail-text);
}
.card .hint {
  color: var(--pinrail-faint);
}
```

The five tones are the ones the manifest's summary uses: `--pinrail-danger`, `--pinrail-warning`, `--pinrail-info`, `--pinrail-success` and `--pinrail-neutral`. `--pinrail-sans` is the app's typeface stack, Inter first, and `--pinrail-mono` its monospace one.

### Layout

`Pinrail.layout()` builds the skeleton the stylesheet expects: a header that stays put, a body that scrolls, and a confirmation bar under the body that stays hidden until the view asks for it. See [The anatomy of a view](#the-anatomy-of-a-view).

```js
const view = Pinrail.layout({ title: "5 tickets", meta: ["acme-api"], controls: [closeAll] });
view.content.innerHTML = rows;               // re-render the body freely
view.title("4 tickets").meta(["acme-api"]);  // the header keeps its listeners
```

When a hand-over needs a second look, such as items left undecided, hold the first one back, say why in the confirmation bar, and let the next hand-over through. Say on the hand-over button what will happen:

```js
plugin.handOverLabel(`Hand over with ${n} undecided`);
view.confirmation({ text: `${n} left undecided. Hand over again to confirm.`, actions: [keepDeciding] });
view.confirmation(null);  // nothing left to confirm
```

The bar is a warning unless you pass `tone: "info"` or `tone: "danger"`. Its actions are buttons of your own, such as one that takes the person back to deciding.

A view that builds its own layout places the same bar itself, below the part that scrolls:

```js
const bar = Pinrail.confirmationBar();
document.getElementById("app").append(bar.element);
bar.confirmation({ text: "2 left undecided. Hand over again to confirm." });
```

| Class | What it is |
|---|---|
| `.pinrail-layout` | The whole view: the header, then the scrolling body. |
| `.pinrail-header` | The bar at the top, with `.pinrail-title`, `.pinrail-meta` and `.pinrail-controls`, pushed to the right. |
| `.pinrail-scroll`, `.pinrail-content` | The body that scrolls, and its padded content. |
| `.pinrail-confirmation` | The bar under the body that `view.confirmation()` fills, with `.pinrail-confirmation-warning`, `-info` or `-danger`. |
| `.pinrail-subhead` | A heading in the body that stays while its section is on screen. |
| `.pinrail-spacer` | Pushes what follows it to the end of a row. |

### Pieces

| Class | What it is |
|---|---|
| `.pinrail-item`, with `.pinrail-item-head`, `-id`, `-title`, `-body` and `-controls` | One thing the person says yes or no to. |
| `.pinrail-btn`, with `.pinrail-btn-primary`, `-danger` or `-ghost` | Buttons. `aria-pressed="true"` marks the chosen one of a set. |
| `.pinrail-field`, `.pinrail-note` | Inputs and text areas. |
| `.pinrail-notice`, with `.pinrail-notice-success`, `-warning` or `-danger` | Something to tell the person. |
| `.pinrail-tone`, with `.pinrail-tone-danger`, `-warning`, `-info`, `-success` or `-neutral` | A label in one of the five tones, such as a severity. |
| `.pinrail-chip` | A small monospace chip: a branch, a line number, an id. |
| `.pinrail-eyebrow`, `.pinrail-dim`, `.pinrail-faint` | A small label above a section, and quieter text. |
| `.pinrail-empty` | What to show when there is nothing to show. |
| `.pinrail-errors` | Errors to show as they came. |

```html
<div class="pinrail-item">
  <div class="pinrail-item-head">
    <span class="pinrail-item-id">#101</span>
    <span class="pinrail-tone pinrail-tone-warning">major</span>
    <span class="pinrail-item-title">Export times out past 50k rows</span>
  </div>
  <div class="pinrail-item-body">Seen in three tickets this week.</div>
  <div class="pinrail-item-controls">
    <button class="pinrail-btn" aria-pressed="true">Close</button>
    <button class="pinrail-btn">Keep</button>
  </div>
</div>
```

## Themes

The app is dark or light, and the view follows it. Before the view paints, the SDK sets `data-theme="dark"` or `data-theme="light"` on your root element, from the frame's address, so the first frame is already right. When the person switches, the app sends `appearance` and the SDK updates it again.

Styles written against the tokens need nothing more. For anything else, key it on the attribute:

```css
.stage {
  background: #101216;
}
[data-theme="light"] .stage {
  background: #f4f5f7;
}
```

`onAppearance(theme)` tells your code when the theme changes, for a canvas or a chart that draws its own colours.

## Icons

A plugin brings its own icons. A view without a build keeps them as SVG files in `view/icons/`, and `Pinrail.icon(name)` draws `icons/<name>.svg`.

```js
button.innerHTML = `${Pinrail.icon("check")} Accept`;
Pinrail.icon("triangle-alert", { size: 16, label: "Warning" });
```

The app draws with [Lucide](https://lucide.dev/icons), so its icons fit best: copy the ones you use from the `lucide-static` package, keeping the licence comment each file starts with. An icon takes the colour of the text around it, in either theme, drawn from its shapes alone. It follows the font size unless you give `size`; give `label` when the icon means something on its own, so screen readers say it.

A view with a build imports its icons from its framework's Lucide package instead, as the React, Vue, Svelte and Vite templates do: `lucide-react`, `@lucide/vue`, `@lucide/svelte`, or `lucide` for plain TypeScript. The build keeps only the icons the view imports, and the SDK stylesheet sizes each `svg.lucide` to the text.

The manifest's `icon` is an SVG file in the folder too, `icon.svg` in the scaffold, which the app shows wherever it names the plugin, drawn the same way. It is at most 32 KB. If the icon cannot be loaded, the app shows the plugin without an icon.

## Markdown

To render Markdown, load the SDK's Markdown script after the SDK:

```html
<script src="/sdk/v1/pinrail-plugin.js"></script>
<script src="/sdk/v1/markdown.js"></script>
```

`Pinrail.markdown(text)` returns CommonMark as HTML, with headings, tables, block quotes, nested lists and code, and `Pinrail.markdownInline(text)` renders one line without a paragraph around it. Raw HTML in the source is escaped rather than passed through, because a view's frame runs inline scripts. A link whose scheme is not `http`, `https` or `mailto` keeps its text but loses its address, and the app asks the person before it opens any other link in their browser. The output has no classes, and the stylesheet styles its plain elements.

## Size

The app gives the view the whole height of the review panel, whatever the view holds, so the hand-over button is in the same place for every plugin. A view whose content is taller scrolls inside its frame. `Pinrail.layout()` builds a header that stays in place over a body that scrolls.

## Your own fonts, styles and scripts

A view loads nothing from the network, but everything in the plugin folder is served beside it. Put fonts, stylesheets, scripts and images in the folder, in an `assets/` folder for instance, and refer to them by relative path:

```html title="view/index.html"
<link rel="stylesheet" href="/sdk/v1/pinrail-plugin.css">
<link rel="stylesheet" href="assets/view.css">
<script type="module" src="assets/view.js"></script>
```

```css title="view/assets/view.css"
@font-face {
  font-family: "Fraunces";
  src: url("fonts/fraunces.woff2") format("woff2");
}
.title {
  font-family: "Fraunces", var(--pinrail-sans);
}
```

A library goes in the same way: bundle it with your view, or copy its built file into the folder. A plugin with a build step, such as Vite, writes its output into the folder and declares the command as `build` in the manifest (see [Building with a framework](/docs/building/frameworks/)); the [Markdown review](/docs/plugins/markdown/) plugin is built with Vite, which bundles markdown-it and Mermaid into its view.

:::caution[Nothing from elsewhere]
A web font from Google Fonts, a script from a CDN, or an image from a URL does not load: a view's frame can reach only its own plugin folder and the SDK. Copy what you need into the folder.
:::
