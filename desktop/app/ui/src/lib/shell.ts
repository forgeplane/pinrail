// Text a shell, or an agent that reads paths the way a shell does, takes
// as it is.

import { onWindows } from "./platform";

/** A path as a shell would read it: quoted when it has a space or a
 *  character a shell treats specially. On Windows, as Windows Terminal
 *  drops one: in double quotes when it has a space or a character cmd
 *  reads, which a Windows path cannot hold a `"` among. */
export function quotePath(path: string, windows = onWindows): string {
  if (windows) return /[\s&|<>^;,()'`]/.test(path) ? `"${path}"` : path;
  return /^[\w@%+=:,./-]+$/.test(path) ? path : `'${path.replaceAll("'", `'\\''`)}'`;
}
