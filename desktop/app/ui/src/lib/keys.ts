// How shortcuts are shown: the platform's modifier glyph, and whether a
// keyboard event carries it.

export const isMac = /Mac/i.test(typeof navigator === "undefined" ? "" : navigator.platform);

/** "⌘" on macOS, "Ctrl" elsewhere. */
export const MOD = isMac ? "⌘" : "Ctrl";
/** "⌥" on macOS, "Alt" elsewhere. */
export const ALT = isMac ? "⌥" : "Alt";
/** "⇧" on macOS, "Shift" elsewhere. */
export const SHIFT = isMac ? "⇧" : "Shift";
/** A combination written in running text: "⌘⇧L" on macOS, "Ctrl+Shift+L" elsewhere. */
export const combo = (...keys: string[]) => keys.join(isMac ? "" : "+");
/** The modifiers a global shortcut needs, as the platform names them. */
export const GLOBAL_MODIFIERS = isMac ? "⌘, ⌃ or ⌥" : "Ctrl, Alt or Super";
/** What showing a file in the system's file browser is called here. */
export const REVEAL = isMac ? "Show in Finder" : "Show in the file manager";
/** Where the app's icon lives while its window is closed. */
export const TRAY = isMac ? "the menu bar" : "the tray";

/** True while a modal dialog is open: the screens behind it keep their
 *  keys to themselves until it closes. */
export const modalOpen = () => document.querySelector('[role="dialog"][aria-modal="true"]') !== null;

/** True when the event carries the platform's primary modifier. */
export const hasMod = (event: KeyboardEvent) => (isMac ? event.metaKey : event.ctrlKey);

/** Whether a key press belongs to an input method's composition, such as the
 *  Enter that confirms a Japanese or Chinese candidate: it is the field's,
 *  not a shortcut. WebKit marks such a press with the key code 229. */
export function composing(event: KeyboardEvent | { nativeEvent: KeyboardEvent }) {
  const native = "nativeEvent" in event ? event.nativeEvent : event;
  return native.isComposing || native.keyCode === 229;
}

/** Whether a key press is typed into a field: a text box, a text area, a
 *  select or editable text, where a letter is the field's and not a
 *  shortcut. */
export function isTyping(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return (
    !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable)
  );
}
