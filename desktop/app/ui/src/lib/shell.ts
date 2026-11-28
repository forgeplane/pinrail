// Text a shell, or an agent that reads paths the way a shell does, takes
// as it is.

import { onWindows } from "./platform";

/** A path as a shell would read it: quoted when it has a space or a
 *  character a shell treats specially. On Windows, as Windows Terminal
 *  drops one: in double quotes unless it is plain, letters, digits and
 *  `_ . : \ / ~ + -`, as cmd and PowerShell read other characters, and a
 *  Windows path cannot hold a `"`. PowerShell still reads a `$name` inside
 *  double quotes, which no quoting cmd also takes would stop. */
export function quotePath(path: string, windows = onWindows): string {
  if (windows) return /^[\w.:\\/~+-]+$/.test(path) ? path : `"${path}"`;
  return /^[\w@%+=:,./-]+$/.test(path) ? path : `'${path.replaceAll("'", `'\\''`)}'`;
}
