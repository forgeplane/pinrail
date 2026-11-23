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

/** A path with the home folder as ~. */
export const short = (folder: string) => folder.replace(/^\/(?:Users|home)\/[^/]+/, "~");

/** A folder's path for the menu: a long one keeps its start and its last
 *  two folders, with an ellipsis between them. */
export const shortPath = (folder: string, max = 40) => {
  const path = short(folder);
  if (path.length <= max) return path;
  const parts = path.split("/");
  const end = parts.slice(-2).join("/");
  const start = parts[0] === "" ? `/${parts[1]}` : parts[0];
  const kept = `${start}/…/${end}`;
  return kept.length < path.length ? kept : path;
};
