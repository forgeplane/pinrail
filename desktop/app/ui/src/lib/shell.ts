// Text a shell, or an agent that reads paths the way a shell does, takes
// as it is.

/** A path as a shell would read it: quoted when it has a space or a
 *  character a shell treats specially. */
export function quotePath(path: string): string {
  return /^[\w@%+=:,./-]+$/.test(path) ? path : `'${path.replaceAll("'", `'\\''`)}'`;
}
