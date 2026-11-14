// Which keys reach the app while a terminal has focus. Programs in a
// terminal never receive Cmd on macOS, so Cmd combinations stay with the
// app. On Linux the programs need Ctrl, so only Ctrl+Shift combinations
// reach the app. Ctrl+` and MOD+Shift+T, which leave the terminal, always
// do.

type Keys = Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey">;

/** Whether a key toggles a session between Summary and Terminal. */
export function isViewToggle(event: Keys, mac: boolean): boolean {
  if (event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey && event.code === "Backquote") return true;
  const mod = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  return mod && event.shiftKey && !event.altKey && event.code === "KeyT";
}

/** Whether a key pressed in a terminal goes on to the app's shortcuts. */
export function reachesApp(event: Keys, mac: boolean): boolean {
  if (isViewToggle(event, mac)) return true;
  if (mac) return event.metaKey;
  return event.ctrlKey && event.shiftKey && !event.metaKey;
}
