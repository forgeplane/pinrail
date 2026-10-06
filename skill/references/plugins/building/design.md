# Design

A view looks like part of the app when it uses the app's stylesheet,
`/sdk/v1/pinrail-plugin.css`. It gives the base styles, the colours and the
classes, in the light and the dark theme. `/sdk/v1/tokens.css` gives the
colours alone. Every rule is in the cascade layer `pinrail`, so any rule of
the view's own wins.

## Colours and type

Style with the tokens, never with colour values, and the view follows the
app's theme.

- Colours: `--pinrail-` followed by `bg`, `bg-panel`, `bg-raised`,
  `bg-hover`, `border`, `border-strong`, `text`, `dim`, `faint`, `accent`,
  `accent-bg` or `button-bg`. The tones are `danger`, `warning`, `info`,
  `success` and `neutral`, and diffs have `add-bg`, `add-gut`, `del-bg`
  and `del-gut`.
- Type: `--pinrail-sans` and `--pinrail-mono`, with text at 13px.
- Themes: the root element has `data-theme="dark"` or `"light"`. Key any
  colour of your own on it. `onAppearance(theme)` says when it changes.

## Layout and classes

- `Pinrail.layout({ title, meta, controls })` builds a fixed
  `.pinrail-header` over a scrolling `.pinrail-content`. Render into
  `layout.content`.
- Items: `.pinrail-item`, with `.pinrail-item-head`, `-id`, `-title`,
  `-body` and `-controls`.
- Buttons: `.pinrail-btn`, with `-primary`, `-danger` or `-ghost`. Mark the
  chosen one with `aria-pressed="true"`.
- Fields and notices: `.pinrail-field`, `.pinrail-note`, and
  `.pinrail-notice` with `-success`, `-warning` or `-danger`.
- Text: `.pinrail-tone` with a tone, such as `.pinrail-tone-warning`, and
  `.pinrail-chip`, `.pinrail-eyebrow`, `.pinrail-dim`, `.pinrail-faint`,
  `.pinrail-empty` and `.pinrail-errors`.

The app draws the review's title and the hand-over button, so the view
draws neither. Keep the view dense and quiet, with one accent colour and
lines rather than boxes. A decided review shows what was there and what
was decided, without controls.

## Icons, fonts and images

- Without a build, `Pinrail.icon(name)` draws `view/icons/<name>.svg` in
  the text's colour. Bring the icons you use. The app's icons are Lucide,
  from `lucide-static`, and fit best.
- With a build, import icons from `lucide-react`, `@lucide/vue`,
  `@lucide/svelte`, or `lucide` with
  `createElement(icon, { class: "lucide" })`. The stylesheet sizes an
  `svg.lucide` to the text.
- The plugin's own icon is `icon.svg` in the folder, either way.
- Put fonts, styles and images of your own in the plugin folder, and refer
  to them by relative path.
