// How shortcuts are shown: the platform's modifier glyph, and whether a
// keyboard event carries it.

export const isMac = /Mac/i.test(typeof navigator === "undefined" ? "" : navigator.platform);

/** "⌘" on macOS, "Ctrl" elsewhere. */
export const MOD = isMac ? "⌘" : "Ctrl";

/** True while a modal dialog is open: the screens behind it keep their
 *  keys to themselves until it closes. */
export const modalOpen = () => document.querySelector('[role="dialog"][aria-modal="true"]') !== null;

/** True when the event carries the platform's primary modifier. */
export const hasMod = (event: KeyboardEvent) => (isMac ? event.metaKey : event.ctrlKey);
