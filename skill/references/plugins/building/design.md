# Design

`/sdk/v1/pinrail-plugin.css` gives the app's base, colours and classes in
both themes; `/sdk/v1/tokens.css` gives the colours alone. Style with the
tokens, never with colours, and the view follows the app's theme. Every
rule is in the cascade layer `pinrail`, so any rule of the view's wins.

- Colours: `--pinrail-` then `bg`, `bg-panel`, `bg-raised`, `bg-hover`,
  `border`, `border-strong`, `text`, `dim`, `faint`, `accent`,
  `accent-bg`, `button-bg`; the tones `danger`, `warning`, `info`,
  `success`, `neutral`; for diffs `add-bg`, `add-gut`, `del-bg`, `del-gut`.
- Type: `--pinrail-sans` and `--pinrail-mono`; text at 13px.
- Layout: `Pinrail.layout({ title, meta, controls })` builds
  `.pinrail-header` and a scrolling `.pinrail-content`; render into
  `layout.content`.
- Pieces: `.pinrail-item` with `.pinrail-item-head`, `-id`, `-title`,
  `-body`, `-controls`; `.pinrail-btn` with `-primary`, `-danger`,
  `-ghost`, and `aria-pressed="true"` on the chosen one;
  `.pinrail-field`, `.pinrail-note`; `.pinrail-notice` with `-success`,
  `-warning`, `-danger`; `.pinrail-tone` with a tone, such as
  `.pinrail-tone-warning`; `.pinrail-chip`, `.pinrail-eyebrow`,
  `.pinrail-dim`, `.pinrail-faint`, `.pinrail-empty`, `.pinrail-errors`.
- Themes: the root element has `data-theme="dark"` or `"light"`; key any
  colour of your own on it. `onAppearance(theme)` says when it changes.
- Icons, without a build: `Pinrail.icon(name)` draws `icons/<name>.svg`
  from `view/icons/`, in the text's colour. Bring the ones you use; the
  app's are Lucide, from `lucide-static`, which fit best.
- Icons, with a build: import them from `lucide-react`, `@lucide/vue`,
  `@lucide/svelte`, or `lucide` (`createElement(icon, { class: "lucide" })`);
  the stylesheet sizes an `svg.lucide` to the text.
- The manifest's `icon` is an SVG file in the folder, either way.
- The app draws the title and the hand-over: the view draws neither.
- Dense and quiet: one accent, lines rather than boxes. A decided review
  shows what was there and what was decided, without controls.
- Fonts, styles and images of your own go in the plugin folder, by
  relative path.
