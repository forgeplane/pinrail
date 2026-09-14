import { MOD, isMac } from "./keys";

/** The in-app shortcuts, for the ? dialog and the settings. */
export const SHORTCUTS: { what: string; keys: string[][] }[] = [
  { what: "Next / previous review", keys: [["J"], ["K"]] },
  { what: "Open the focused review", keys: [["Enter"]] },
  { what: "Discard the focused review", keys: [["D"]] },
  { what: "Search the inbox or the history", keys: [["/"]] },
  { what: "Search everything", keys: [[MOD, "K"]] },
  { what: "Inbox", keys: [[MOD, "I"]] },
  { what: "History / Plugins", keys: [[MOD, "⇧", "H"], [MOD, "⇧", "P"]] },
  { what: "Settings", keys: [[MOD, ","]] },
  { what: "Switch theme", keys: [["T"]] },
  { what: "Show or hide the sidebar", keys: [[MOD, "B"]] },
  { what: "Back / forward", keys: [[MOD, "["], [MOD, "]"]] },
  { what: "Hand over to the agent", keys: [[MOD, "Enter"]] },
  { what: "Maximize / restore the view", keys: [[MOD, "⇧", "M"]] },
  { what: "Previous / next round", keys: [["["], ["]"]] },
  { what: "Keyboard shortcuts", keys: [["?"]] },
];

/** The global shortcut as the core has it when nobody changed it. */
export const DEFAULT_GLOBAL_SHORTCUT = "alt+shift+w";

const MODIFIER_GLYPHS: Record<string, [string, string]> = {
  cmd: ["⌘", "Win"],
  command: ["⌘", "Win"],
  super: ["⌘", "Win"],
  meta: ["⌘", "Win"],
  ctrl: ["⌃", "Ctrl"],
  control: ["⌃", "Ctrl"],
  alt: ["⌥", "Alt"],
  option: ["⌥", "Alt"],
  shift: ["⇧", "Shift"],
  cmdorctrl: [MOD, MOD],
  commandorcontrol: [MOD, MOD],
};

const KEY_GLYPHS: Record<string, string> = {
  arrowup: "↑",
  arrowdown: "↓",
  arrowleft: "←",
  arrowright: "→",
  up: "↑",
  down: "↓",
  left: "←",
  right: "→",
  space: "Space",
  enter: "↩",
  return: "↩",
  escape: "Esc",
  backspace: "⌫",
  delete: "⌦",
  tab: "⇥",
  comma: ",",
  period: ".",
  slash: "/",
  backslash: "\\",
  semicolon: ";",
  quote: "'",
  bracketleft: "[",
  bracketright: "]",
  minus: "-",
  equal: "=",
  backquote: "`",
};

/** A shortcut string ("alt+shift+w") as the glyphs a row shows. */
export function shortcutGlyphs(shortcut: string): string[] {
  const parts = shortcut.split("+").map((p) => p.trim()).filter(Boolean);
  return parts.map((part) => {
    const lower = part.toLowerCase();
    const modifier = MODIFIER_GLYPHS[lower];
    if (modifier) return isMac ? modifier[0] : modifier[1];
    if (KEY_GLYPHS[lower]) return KEY_GLYPHS[lower];
    const named = /^(key|digit)(.)$/.exec(lower);
    if (named) return named[2].toUpperCase();
    return part.length === 1 ? part.toUpperCase() : part;
  });
}

const MODIFIER_CODES = /^(Shift|Control|Alt|Meta|OS)(Left|Right)?$/;

/**
 * The shortcut a key press asks for, in the form the app registers, or
 * null when the press is a modifier alone or carries none: a global
 * shortcut needs ⌘, ⌃ or ⌥ so it does not steal plain typing elsewhere.
 */
export function shortcutFromEvent(event: KeyboardEvent): string | null {
  if (MODIFIER_CODES.test(event.code) || !event.code) return null;
  if (!(event.metaKey || event.ctrlKey || event.altKey)) return null;
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("ctrl");
  if (event.altKey) parts.push("alt");
  if (event.shiftKey) parts.push("shift");
  if (event.metaKey) parts.push("cmd");
  const named = /^(?:Key|Digit)(.)$/.exec(event.code);
  parts.push(named ? named[1].toLowerCase() : event.code);
  return parts.join("+");
}
