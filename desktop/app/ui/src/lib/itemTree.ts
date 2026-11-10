// The rows of a list of work items: each sub-item under its parent, in the
// list's order, or every item as a row of its own.

import type { WorkItem } from "../api/types";

/** An item as a row, at its depth in the tree. */
export type ItemTreeRow = { item: WorkItem; depth: number; folded: boolean; children: number };

/** The rows of `list`. Nested, a sub-item goes under its parent when the
 *  parent is in the list, and stays at the top level when it is not; a
 *  folded parent hides every row under it. */
export function itemTree(list: WorkItem[], { nested, folded }: { nested: boolean; folded: Set<string> }) {
  if (!nested) return list.map((item): ItemTreeRow => ({ item, depth: 0, folded: false, children: 0 }));
  const here = new Set(list.map((i) => i.key));
  const children = new Map<string, WorkItem[]>();
  const roots: WorkItem[] = [];
  for (const item of list) {
    if (item.parent && here.has(item.parent)) {
      const kids = children.get(item.parent);
      if (kids) kids.push(item);
      else children.set(item.parent, [item]);
    } else roots.push(item);
  }
  const out: ItemTreeRow[] = [];
  const walk = (item: WorkItem, depth: number) => {
    const kids = children.get(item.key) ?? [];
    const isFolded = folded.has(item.key);
    out.push({ item, depth, folded: isFolded, children: kids.length });
    if (!isFolded) for (const kid of kids) walk(kid, depth + 1);
  };
  for (const root of roots) walk(root, 0);
  return out;
}

/** The rows of one page of grouped rows: the top-level rows from
 *  `(page - 1) * size` on, `size` of them, each with the rows under it,
 *  in groups that keep their order. A group with no row on the page is
 *  left out. `total` is the number of top-level rows in every group. */
export function pageOfGroups<K>(groups: [K, ItemTreeRow[]][], page: number, size: number) {
  const start = (page - 1) * size;
  const end = start + size;
  let top = -1;
  const shown: [K, ItemTreeRow[]][] = [];
  for (const [key, rows] of groups) {
    const here = rows.filter((row) => {
      if (row.depth === 0) top++;
      return top >= start && top < end;
    });
    if (here.length) shown.push([key, here]);
  }
  return { groups: shown, total: top + 1 };
}
