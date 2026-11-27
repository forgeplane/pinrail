// Names in a sentence: "A", "A and B", "A, B and C".

import { Fragment, type ReactNode } from "react";

export function listed(names: string[], strong = false): ReactNode {
  return names.map((name, i) => (
    <Fragment key={name}>
      {i === 0 ? "" : i === names.length - 1 ? " and " : ", "}
      {strong ? <b>{name}</b> : name}
    </Fragment>
  ));
}

/** A path with the home folder as ~: /Users/p, /home/p or C:\Users\p. */
export const short = (folder: string) => folder.replace(/^(?:\/(?:Users|home)\/[^/]+|[A-Za-z]:\\Users\\[^\\]+)/i, "~");

/** A folder's path for the menu: a long one keeps its start and its last
 *  two folders, with an ellipsis between them, with the path's own
 *  separator, \ on Windows. */
export const shortPath = (folder: string, max = 40) => {
  const path = short(folder);
  if (path.length <= max) return path;
  const sep = path.includes("\\") && !path.includes("/") ? "\\" : "/";
  const parts = path.split(sep);
  const end = parts.slice(-2).join(sep);
  const start = parts[0] === "" ? `${sep}${parts[1]}` : parts[0];
  const kept = `${start}${sep}…${sep}${end}`;
  return kept.length < path.length ? kept : path;
};
