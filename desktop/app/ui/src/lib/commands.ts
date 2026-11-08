// The app's commands, defined once: what the command palette calls them, the
// keys that run them as the ? dialog shows them, which key presses are
// theirs, and whether the native menu sends them by id. The layout says what
// each one does; the palette, the key handler and the menu handler are all
// built from this table.

import { ALT, MOD, SHIFT, hasMod } from "./keys";

export type CommandId =
  | "search"
  | "go-inbox"
  | "go-sessions"
  | "go-items"
  | "go-projects"
  | "go-history"
  | "go-plugins"
  | "next-waiting"
  | "previous-waiting"
  | "add-item"
  | "settings"
  | "toggle-theme"
  | "toggle-sidebar"
  | "back"
  | "forward"
  | "shortcuts"
  | "whats-new"
  | "feedback";

export type Command = {
  id: CommandId;
  /** what the command palette calls it; none keeps it out of the palette */
  label?: string;
  /** the keys that run it, as the ? dialog and the palette show them */
  keys?: string[][];
  /** whether a key press is this command's */
  matches?: (event: KeyboardEvent) => boolean;
  /** the native menu sends it, as a command with its id */
  menu?: boolean;
};

/** ⌘ (Ctrl elsewhere) with `key`, and ⇧ when `shift`, with no other modifier. */
const withMod =
  (key: string, shift = false) =>
  (event: KeyboardEvent) =>
    hasMod(event) && !event.altKey && event.shiftKey === shift && event.key.toLowerCase() === key;

/** ⌥ with an arrow, and no other modifier. */
const withAlt = (key: string) => (event: KeyboardEvent) =>
  event.altKey && !hasMod(event) && !event.shiftKey && event.key === key;

export const COMMANDS: Command[] = [
  { id: "search", keys: [[MOD, "K"]], matches: withMod("k"), menu: true },
  {
    id: "go-inbox",
    label: "Go to inbox",
    keys: [
      [MOD, "1"],
      [MOD, "I"],
    ],
    matches: (e) => withMod("1")(e) || withMod("i")(e),
    menu: true,
  },
  { id: "go-sessions", label: "Go to sessions", keys: [[MOD, "2"]], matches: withMod("2"), menu: true },
  { id: "go-items", label: "Go to work items", keys: [[MOD, "3"]], matches: withMod("3"), menu: true },
  { id: "go-projects", label: "Go to projects", keys: [[MOD, "4"]], matches: withMod("4"), menu: true },
  { id: "go-history", label: "Go to history", keys: [[MOD, SHIFT, "H"]], matches: withMod("h", true), menu: true },
  { id: "go-plugins", label: "Plugins", keys: [[MOD, SHIFT, "P"]], matches: withMod("p", true), menu: true },
  { id: "next-waiting", keys: [[ALT, "↓"]], matches: withAlt("ArrowDown") },
  { id: "previous-waiting", keys: [[ALT, "↑"]], matches: withAlt("ArrowUp") },
  { id: "add-item", label: "Add a work item", keys: [[MOD, SHIFT, "C"]], matches: withMod("c", true) },
  { id: "settings", label: "Open settings", keys: [[MOD, ","]], matches: withMod(","), menu: true },
  { id: "toggle-theme", label: "Switch theme", keys: [[MOD, SHIFT, "L"]], matches: withMod("l", true), menu: true },
  { id: "toggle-sidebar", label: "Show or hide the sidebar", keys: [[MOD, "B"]], matches: withMod("b"), menu: true },
  { id: "back", keys: [[MOD, "["]], matches: withMod("["), menu: true },
  { id: "forward", keys: [[MOD, "]"]], matches: withMod("]"), menu: true },
  { id: "shortcuts", label: "Keyboard shortcuts", keys: [["?"]] },
  { id: "whats-new", label: "What's new in Pinrail", menu: true },
  { id: "feedback", label: "Send feedback…", menu: true },
];

/** A command by its id. */
export const command = (id: CommandId): Command => COMMANDS.find((c) => c.id === id)!;
