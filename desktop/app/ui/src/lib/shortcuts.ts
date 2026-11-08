import { ALT, MOD, SHIFT, isMac } from "./keys";

/** The sections the shortcuts are listed in, in this order. */
export const SHORTCUT_SECTIONS = ["General", "Inbox", "Review", "Sessions", "Work items"] as const;
export type ShortcutSection = (typeof SHORTCUT_SECTIONS)[number];

export type Shortcut = { what: string; keys: string[][]; section: ShortcutSection };

/** The in-app shortcuts, for the ? dialog and the settings. */
export const SHORTCUTS: Shortcut[] = [
  // anywhere
  { section: "General", what: "Search everything", keys: [[MOD, "K"]] },
  { section: "General", what: "Search the page", keys: [["/"]] },
  {
    section: "General",
    what: "Inbox",
    keys: [
      [MOD, "1"],
      [MOD, "I"],
    ],
  },
  { section: "General", what: "Sessions", keys: [[MOD, "2"]] },
  { section: "General", what: "Work items", keys: [[MOD, "3"]] },
  { section: "General", what: "Projects", keys: [[MOD, "4"]] },
  {
    section: "General",
    what: "History / Plugins",
    keys: [
      [MOD, SHIFT, "H"],
      [MOD, SHIFT, "P"],
    ],
  },
  {
    section: "General",
    what: "Next / previous waiting review",
    keys: [
      [ALT, "↓"],
      [ALT, "↑"],
    ],
  },
  { section: "General", what: "Add a work item from any page", keys: [[MOD, SHIFT, "C"]] },
  { section: "General", what: "Settings", keys: [[MOD, ","]] },
  { section: "General", what: "Switch theme", keys: [[MOD, SHIFT, "L"]] },
  { section: "General", what: "Show or hide the sidebar", keys: [[MOD, "B"]] },
  {
    section: "General",
    what: "Back / forward",
    keys: [
      [MOD, "["],
      [MOD, "]"],
    ],
  },
  { section: "General", what: "Keyboard shortcuts", keys: [["?"]] },
  // the inbox and the history
  { section: "Inbox", what: "Next / previous review", keys: [["J"], ["K"]] },
  { section: "Inbox", what: "Open the focused review", keys: [["Enter"]] },
  { section: "Inbox", what: "Discard the focused or open review", keys: [[MOD, "D"]] },
  // a review, on its page or in a side panel
  { section: "Review", what: "Hand over to the agent", keys: [[MOD, "Enter"]] },
  { section: "Review", what: "Star or remove the star of the review", keys: [[MOD, SHIFT, "S"]] },
  { section: "Review", what: "Maximize / restore the view", keys: [[MOD, SHIFT, "M"]] },
  { section: "Review", what: "Find in the view", keys: [[MOD, "F"]] },
  { section: "Review", what: "Previous / next round", keys: [["["], ["]"]] },
  // sessions and their threads
  { section: "Sessions", what: "Next / previous session", keys: [["J"], ["K"]] },
  { section: "Sessions", what: "Archive or unarchive the session", keys: [["E"]] },
  { section: "Sessions", what: "Pick a choice of the current question", keys: [["1–9"]] },
  { section: "Sessions", what: "Next / previous question", keys: [["N"], ["P"]] },
  { section: "Sessions", what: "Add a note to the answer", keys: [["C"]] },
  { section: "Sessions", what: "Send the answers", keys: [[MOD, "Enter"]] },
  { section: "Sessions", what: "Decline the questions", keys: [[MOD, "D"]] },
  { section: "Sessions", what: "Show or hide a session's details", keys: [[MOD, SHIFT, "B"]] },
  { section: "Sessions", what: "Find in the thread", keys: [[MOD, "F"]] },
  // work items
  { section: "Work items", what: "Next / previous work item", keys: [["J"], ["K"]] },
  { section: "Work items", what: "Open the focused work item", keys: [["Enter"]] },
  { section: "Work items", what: "Add a work item", keys: [["C"]] },
  { section: "Work items", what: "Claim or release the work item", keys: [["S"]] },
  { section: "Work items", what: "Mark the work item done, or open it again", keys: [["D"]] },
  { section: "Work items", what: "Cancel the work item, or open it again", keys: [["X"]] },
  { section: "Work items", what: "Put the work item on hold, or take it off hold", keys: [["H"]] },
  { section: "Work items", what: "Queue the work item, or take it out of the queue", keys: [["Q"]] },
  {
    section: "Work items",
    what: "Move the work item up or down the queue",
    keys: [
      [SHIFT, "↑"],
      [SHIFT, "↓"],
    ],
  },
  { section: "Work items", what: "Edit the work item's title", keys: [["T"]] },
  { section: "Work items", what: "Edit the work item's description", keys: [["E"]] },
];

/** The sections of the page at `path`, with `search` its query string,
 *  its own first and General last. A review open in a side panel adds the
 *  Review section. */
export function sectionsOf(path: string, search: string): ShortcutSection[] {
  const own: ShortcutSection[] = [];
  if (path === "/" || path.startsWith("/history")) own.push("Inbox");
  if (path.startsWith("/sessions")) own.push("Sessions");
  if (path.startsWith("/items")) own.push("Work items");
  const panel = new URLSearchParams(search);
  if (path.startsWith("/reviews/") || panel.has("review") || (own.includes("Inbox") && panel.has("open")))
    own.push("Review");
  return [...own, "General"];
}

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
  ...["k", ",", "i", "b", "d", "f", "[", "]", "q", "w", "m", "z", "x", "c", "v", "a", "1", "2", "3", "4"].map((k) =>
    combo(k, PRIMARY),
  ),
  ...["h", "p", "m", "l", "z", "c"].map((k) => combo(k, PRIMARY, "shift")),
  ...(isMac ? [combo("h", "cmd"), combo("h", "alt", "cmd"), combo("f", "ctrl", "cmd")] : []),
  // the review screen: help, rounds, closing, and handing over. J and K
  // move through the inbox while a review is open in its side panel
  "shift+/",
  "[",
  "]",
  "j",
  "k",
  "shift+j",
  "shift+k",
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
