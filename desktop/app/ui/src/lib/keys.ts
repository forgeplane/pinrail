// How shortcuts are shown: the platform's modifier glyph, and whether a
// keyboard event carries it.

export const isMac = /Mac/i.test(typeof navigator === "undefined" ? "" : navigator.platform);

/** "⌘" on macOS, "Ctrl" elsewhere. */
export const MOD = isMac ? "⌘" : "Ctrl";

/** True when the event carries the platform's primary modifier. */
export const hasMod = (event: KeyboardEvent) => (isMac ? event.metaKey : event.ctrlKey);
