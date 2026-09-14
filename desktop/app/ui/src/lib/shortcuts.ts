import { MOD } from "./keys";

/** The in-app shortcuts, for the ? dialog and the settings. */
export const SHORTCUTS: { what: string; keys: string[][] }[] = [
  { what: "Next / previous review", keys: [["J"], ["K"]] },
  { what: "Open the focused review", keys: [["Enter"]] },
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

/** The shortcut that works outside the app. */
export const GLOBAL_SHORTCUT = ["⌥", "⇧", "W"];
