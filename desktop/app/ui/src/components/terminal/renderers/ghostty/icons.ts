// Nerd Font icons: the private use areas, which prompts, status lines and
// file trees fill with icons from a patched font. The symbols-only Nerd Font
// is bundled for them, and loaded only once a screen shows one.

import fontUrl from "./SymbolsNerdFont-Regular.woff2?url";

/** The bundled font's family, a name of its own, so that an older symbols
 *  font installed on the computer is never used in its place. */
export const ICON_FAMILY = "Pinrail Symbols Nerd Font";

/** Whether a cell's text is one code point of the private use areas:
 *  U+E000 to U+F8FF, or the supplementary areas from U+F0000. */
export function isIcon(text: string): boolean {
  const cp = text.codePointAt(0);
  if (cp === undefined || text.length !== (cp > 0xffff ? 2 : 1)) return false;
  return (cp >= 0xe000 && cp <= 0xf8ff) || cp >= 0xf0000;
}

let state: "idle" | "loading" | "ready" | "failed" = "idle";
let loaded: Promise<void> | null = null;

/** Loads the bundled font, once. Without it, as when the file cannot be
 *  read, icons are drawn from whatever font has them. */
function load(): Promise<void> {
  if (loaded) return loaded;
  state = "loading";
  const face = new FontFace(ICON_FAMILY, `url(${fontUrl})`);
  loaded = face.load().then(
    (ready) => {
      document.fonts.add(ready);
      state = "ready";
    },
    () => {
      state = "failed";
    },
  );
  return loaded;
}

/** Whether icons can be drawn now. Before the font is ready it starts
 *  loading it and calls `then` once it is. */
export function iconFontReady(then: () => void): boolean {
  if (state === "ready" || state === "failed") return true;
  void load().then(then);
  return false;
}

/** Settles once the font has loaded or failed to, for tests. */
export function iconFontLoaded(): Promise<void> {
  return load();
}
