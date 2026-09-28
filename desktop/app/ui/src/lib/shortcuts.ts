import { ALT, MOD, SHIFT, isMac } from "./keys";

/** The in-app shortcuts, for the ? dialog and the settings. */
/** The `repo` filter's value for the reviews that name no project. */
export const NO_PROJECT = "-";

/** Whether a review passes the project filter: none, a project, or `NO_PROJECT`. */
export const inProject = (repo: string | null | undefined, filter: string) =>
  !filter || (filter === NO_PROJECT ? !repo : repo === filter);

export const SHORTCUTS: { what: string; keys: string[][] }[] = [
  { what: "Next / previous review", keys: [["J"], ["K"]] },
  { what: "Open the focused review", keys: [["Enter"]] },
  {
    what: "Next / previous waiting review",
    keys: [
      [ALT, "↓"],
      [ALT, "↑"],
    ],
  },
  { what: "Discard the focused review", keys: [["D"]] },
  { what: "Search the inbox or the history", keys: [["/"]] },
  { what: "Search everything", keys: [[MOD, "K"]] },
  { what: "Inbox", keys: [[MOD, "I"]] },
  {
    what: "History / Plugins",
    keys: [
      [MOD, SHIFT, "H"],
      [MOD, SHIFT, "P"],
    ],
  },
  { what: "Settings", keys: [[MOD, ","]] },
  { what: "Switch theme", keys: [[MOD, SHIFT, "L"]] },
  { what: "Show or hide the sidebar", keys: [[MOD, "B"]] },
  {
    what: "Back / forward",
    keys: [
      [MOD, "["],
      [MOD, "]"],
    ],
  },
  { what: "Hand over to the agent", keys: [[MOD, "Enter"]] },
  { what: "Maximize / restore the view", keys: [[MOD, SHIFT, "M"]] },
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
  const parts = shortcut
    .split("+")
    .map((p) => p.trim())
    .filter(Boolean);
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

const CODE_NAMES: Record<string, string> = {
  Slash: "/",
  BracketLeft: "[",
  BracketRight: "]",
  Comma: ",",
  Period: ".",
  Minus: "-",
  Equal: "=",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Backslash: "\\",
};

/**
 * The combination a key press is, in the form a manifest declares it:
 * modifiers then one key, lowercase, a bare key allowed. Null for a
 * modifier on its own.
 */
export function comboFromEvent(event: KeyboardEvent): string | null {
  if (!event.code || MODIFIER_CODES.test(event.code)) return null;
  const named = /^(?:Key|Digit)(.)$/.exec(event.code);
  const name = named ? named[1].toLowerCase() : (CODE_NAMES[event.code] ?? event.code.toLowerCase());
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("ctrl");
  if (event.altKey) parts.push("alt");
  if (event.shiftKey) parts.push("shift");
  if (event.metaKey) parts.push("cmd");
  parts.push(name);
  return parts.join("+");
}

/** A combination as `comboFromEvent` writes it: ctrl, alt, shift, cmd, then the key. */
const combo = (key: string, ...modifiers: string[]) =>
  [...["ctrl", "alt", "shift", "cmd"].filter((m) => modifiers.includes(m)), key].join("+");
const PRIMARY = isMac ? "cmd" : "ctrl";

/**
 * The keys the app itself answers: its menus (desktop/app/src-tauri/src/lib.rs
 * and the standard Edit, Window and application menus) and the review
 * screen. A plugin that declares one of these is told so, and never
 * receives it; every other declared key is forwarded to its view.
 */
export const APP_KEYS = new Set([
  ...["k", ",", "i", "b", "[", "]", "q", "w", "m", "z", "x", "c", "v", "a"].map((k) => combo(k, PRIMARY)),
  ...["h", "p", "m", "l", "z"].map((k) => combo(k, PRIMARY, "shift")),
  ...(isMac ? [combo("h", "cmd"), combo("h", "alt", "cmd"), combo("f", "ctrl", "cmd")] : []),
  // the review screen: help, rounds, closing, and handing over
  "shift+/",
  "[",
  "]",
  "escape",
  "cmd+enter",
  "ctrl+enter",
]);
export const isShadowed = (keys: string) => APP_KEYS.has(keys);

/**
 * The shortcut a key press asks for, in the form the app registers, or
 * null when the press is a modifier alone or carries none: a global
 * shortcut needs a modifier (⌘, ⌃ or ⌥; Ctrl, Alt or Super on Linux) so it
 * does not steal plain typing elsewhere.
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
