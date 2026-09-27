---
title: Design and styling
description: "How a view looks like part of Pinrail: the design language, the SDK stylesheet's tokens and classes, themes, type, icons, and bringing your own fonts, styles and scripts."
---

A view sits inside the app, between the review's header and the hand-over button, so the person reads it as part of Pinrail. This page is how to make it look that way with little effort: the conventions the built-in plugins follow, and what the SDK stylesheet gives you.

:::tip[Your view, your style]
None of this is required. The stylesheet is a set of defaults, and your own styles come after it and win. Use its tokens and ignore its classes, restyle everything, or bring a design system of your own, as long as it ships inside the plugin folder (see [Your own fonts, styles and scripts](#your-own-fonts-styles-and-scripts)).
:::

## The design language

What makes a view feel native, whatever it draws:

- **The app owns the hand-over.** Draw no submit button and no title bar: the app puts the review's title above the view and the hand-over button below it, in the same place for every plugin. Tell that button what it will do with `plugin.status`, such as *Hand over 3 of 5*.
- **Dense and quiet.** Text at 13px, one accent colour, lines rather than boxes. What matters is the work being reviewed, not the frame around it.
- **Keys for the common path.** Most plugins move with <kbd>j</kbd> and <kbd>k</kbd> and give a verdict with one key each. Declare them in the manifest so the app lists them in its keyboard help (see [Settings and keys](/docs/building/settings-and-keys/)).
- **Read-only is a full view.** A decided review is read months later: render what was there and what was decided, without the controls.
- **A brand's colours stay in the content.** A plugin that shows something in its own palette, as the Logo plugin shows marks in the brand's colours, paints that inside its content; the chrome around it keeps the app's tokens.

## The stylesheet

```html
<link rel="stylesheet" href="/sdk/v1/pinrail-plugin.css">
```

It sets the base (type, links, focus rings, code, tables, narrow scrollbars), the colour tokens below in both themes, and a few classes for the shapes most views need. It also styles what `Pinrail.markdown` renders: headings, lists, quotes, tables and code.

### Colour tokens

Style with the tokens rather than with colours, and your view follows the app in both themes with no palette of its own to keep in step.

![The SDK stylesheet's colour tokens](tokens:)

```css
.card {
  background: var(--bg-raised);
  border: 1px solid var(--border);
  color: var(--text);
}
.card .hint {
  color: var(--faint);
}
```

`--sans` is the app's typeface stack, Inter first, and `--mono` its monospace one.

### Layout

`Pinrail.layout()` builds the skeleton the stylesheet expects: a header that stays put and a body that scrolls.

```js
const view = Pinrail.layout({ title: "5 tickets", meta: ["acme-api"], controls: [closeAll] });
view.content.innerHTML = rows;               // re-render the body freely
view.title("4 tickets").meta(["acme-api"]);  // the header keeps its listeners
```

| Class | What it is |
|---|---|
| `.plugin-layout` | The whole view: the header, then the scrolling body. |
| `.plugin-header` | The bar at the top, with `.plugin-title`, `.plugin-meta` and `.plugin-controls`, pushed to the right. |
| `.plugin-scroll`, `.plugin-content` | The body that scrolls, and its padded content. |
| `.plugin-subhead` | A heading in the body that stays while its section is on screen. |
| `.plugin-footer` | A bar that stays at the bottom. |
| `.spacer` | Pushes what follows it to the end of a row. |

### Pieces

| Class | What it is |
|---|---|
| `.item`, with `.head`, `.id`, `.title`, `.body`, `.controls` | One thing the person says yes or no to. |
| `.btn`, `.btn.primary`, `.btn.danger`, `.btn.ghost` | Buttons. `aria-pressed="true"` marks the chosen one of a set. |
| `.field`, `.note` | Inputs and text areas. |
| `.notice.ok`, `.notice.warn`, `.notice.danger` | Something to tell the person, in the app's three tones. |
| `.sev.sev-blocker`, `.sev-major`, `.sev-minor`, `.sev-nit` | Severity chips, in the app's four levels. |
| `.meta` | A small monospace chip: a branch, a line number, an id. |
| `.eyebrow`, `.dim`, `.faint` | A small label above a section, and quieter text. |
| `.empty` | What to show when there is nothing to show. |
| `.errors` | Violations the app sent back, as they came. |

```html
<div class="item">
  <div class="head">
    <span class="id">#101</span>
    <span class="sev sev-major">major</span>
    <span class="title">Export times out past 50k rows</span>
  </div>
  <div class="body">Seen in three tickets this week.</div>
  <div class="controls">
    <button class="btn" aria-pressed="true">Close</button>
    <button class="btn">Keep</button>
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

A plugin brings its own icons, as SVG files in `icons/` beside its view: `view/icons/`, or for a Vite build `src/public/icons/`, which the build copies there. `Pinrail.icon(name)` draws `icons/<name>.svg`.

```js
button.innerHTML = `${Pinrail.icon("check")} Accept`;
Pinrail.icon("triangle-alert", { size: 16, label: "Warning" });
```

The app draws with [Lucide](https://lucide.dev/icons), so its icons fit best: copy the ones you use from the `lucide-static` package, keeping the licence comment each file starts with. An icon takes the colour of the text around it, in either theme, drawn from its shapes alone. It follows the font size unless you give `size`; give `label` when the icon means something on its own, so screen readers say it.

The manifest's `icon` is an SVG file in the folder too, `icon.svg` in the scaffold, which the app shows wherever it names the plugin, drawn the same way. It is at most 32 KB; one that does not load costs the plugin its icon, not its place.

## Size

A view sizes itself to its content by default, and the review page scrolls. A workbench, such as a diff with its own panes, asks for the whole height instead and scrolls inside:

```js
Pinrail.connect({ resize: "fill", onInit, onCollect });
```

`min_height` in the manifest is the least height the app gives the view.

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
  font-family: "Fraunces", var(--sans);
}
```

A library goes in the same way: bundle it with your view, or copy its built file into the folder. A plugin with a build step, such as Vite, writes its output into the folder and declares the command as `build` in the manifest (see [Building with a framework](/docs/building/frameworks/)); the [Artifact](/docs/plugins/artifact/) plugin is built this way, and the [3D model](/docs/plugins/model/) plugin bundles three.js into its view.

:::caution[Nothing from elsewhere]
A web font from Google Fonts, a script from a CDN, or an image from a URL does not load: a view's frame can reach only its own plugin folder and the SDK. Copy what you need into the folder.
:::
