# Input, selection and accessibility checks

These checks cover what the benchmark cannot measure: the renderer spec's
requirements for input, selection, links, accessibility and glyphs. They run
on macOS, in the system WKWebView that the app uses.

There are two parts:

1. An automated run with real AppKit events, which `check.mjs` drives
   through `runners/wkcheck.swift`.
2. Hand checks, for what no driver can produce: dead keys, a real input
   method's candidate window, and VoiceOver.

## The automated checks

```sh
mise run check:terminal                                  # both renderers, about a minute each
mise run check:terminal -- --adapters ghostty
```

The check window takes focus while it runs, so leave the keyboard and mouse
alone until it closes. The pasteboard is saved before the copy and paste
checks and restored after them. The results are written to
`results/checks-<time>.md` and `results/latest-checks.md`, with screenshots
in `results/checks/`.

The run covers:

- keys: letters, Control-C, the arrow keys, Backspace and Return;
- input methods: Japanese, Korean and Chinese text committed, nothing sent
  while composing, and the candidate window at the cursor;
- Option-B with Option as Meta off;
- selection by dragging, and dragging past the top edge into the scrollback;
- Command-C and Command-V, with bracketed paste;
- Command-click on an OSC 8 link and on a plain URL;
- the screen reader's rows and live region;
- the glyphs of ZWJ sequences, flags, VS16 emoji, Powerline and box drawing,
  in screenshots.

AppKit's synthetic mouse events leave the DOM `buttons` field at 0, so a
check that depends on it reports that it could not run rather than failing.

## Hand checks

Each takes a few minutes per renderer. Open the renderer on your login shell
with `live.sh`, and watch the panel on the right, which shows what the
renderer sends:

```sh
desktop/app/ui/bench/terminal/live.sh ghostty           # the screen reader mirror off
desktop/app/ui/bench/terminal/live.sh ghostty --sr      # the screen reader mirror on
```

1. **Dead keys.** On the ABC or US layout, type Option-E then E, Option-U
   then U, and Option-N then N. Expect é, ü and ñ. On the Greek layout, type
   `;` then α, and expect ά.
2. **Input method.** With Japanese (Romaji), type `nihon`, Space, then
   Return. The candidate window must open beside the cursor, and only 日本
   (or the chosen candidate) may be sent. With Korean (2-Set), type `gks`
   and expect 한. With Pinyin, type `nihao`, then 1, and expect 你好. Use
   only the input sources you already have.
3. **Selection.** Double-click a word, triple-click a line, Shift-click to
   extend, and drag past the top edge into the scrollback. Copy the
   selection and paste it into another app.
4. **VoiceOver.** Open the renderer with `--sr`, then turn VoiceOver on with
   Command-F5. Run `ls`, and check that VoiceOver reads the output once.
   Move through the rows with VO-Up and VO-Down, and check that each row is
   read as its text.
