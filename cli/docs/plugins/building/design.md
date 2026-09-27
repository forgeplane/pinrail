---
title: Design
summary: Look like the app: the stylesheet's tokens and classes, themes, icons.
menu: []
---
# Design

`/sdk/v1/pinrail-plugin.css` gives the app's base, colours and classes in
both themes. Style with its tokens, never with colours, and the view
follows the app's theme.

- Colours: `--bg`, `--bg-panel`, `--bg-raised`, `--bg-hover`, `--border`,
  `--border-strong`, `--text`, `--dim`, `--faint`, `--accent`,
  `--accent-bg`, `--button-bg`, `--ok`, `--danger`, `--sev-blocker`,
  `--sev-major`, `--sev-minor`, `--sev-nit`, and for diffs `--add-bg`,
  `--add-gut`, `--del-bg`, `--del-gut`.
- Type: `--sans` and `--mono`; text at 13px.
- Layout: `Pinrail.layout({ title, meta, controls })` builds
  `.plugin-header` and a scrolling `.plugin-content`; render into
  `layout.content`.
- Pieces: `.item` with `.head`, `.id`, `.title`, `.body`, `.controls`;
  `.btn`, `.btn.primary`, `.btn.danger`, `.btn.ghost`, with
  `aria-pressed="true"` on the chosen one; `.field`, `.note`;
  `.notice.ok`, `.notice.warn`, `.notice.danger`; `.sev.sev-major` and the
  other levels; `.meta`, `.eyebrow`, `.dim`, `.faint`, `.empty`, `.errors`.
- Themes: the root element has `data-theme="dark"` or `"light"`; key any
  colour of your own on it. `onAppearance(theme)` says when it changes.
- Icons: `Pinrail.icon(name)` draws `icons/<name>.svg` beside the view
  (for a Vite build, `src/public/icons/`), in the text's colour. Bring the
  ones you use; the app's are Lucide, from `lucide-static`, which fit best.
  The manifest's `icon` is an SVG file in the folder too.
- The app draws the title and the hand-over: the view draws neither.
- Dense and quiet: one accent, lines rather than boxes. A decided review
  shows what was there and what was decided, without controls.
- Fonts, styles and images of your own go in the plugin folder, by
  relative path.
