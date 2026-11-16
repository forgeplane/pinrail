// The orders the Work items page lists its items in. The groups and the
// sub-items under each parent keep the order they are given.

import type { WorkItem } from "../api/types";

export type Ordering = "oldest" | "newest" | "updated" | "status" | "title";

export const ORDERINGS: { value: Ordering; label: string }[] = [
  { value: "oldest", label: "Oldest first" },
  { value: "newest", label: "Newest first" },
  { value: "updated", label: "Recently updated" },
  { value: "status", label: "Status" },
  { value: "title", label: "Title" },
];

/** In progress first, then what is ready, blocked or waiting, then what is closed. */
const standing = (i: WorkItem) =>
  i.status === "in_progress"
    ? 0
    : i.status === "open"
      ? i.ready
        ? 1
        : i.blocked
          ? 2
          : 3
      : i.status === "done"
        ? 4
        : 5;

/** Titles compared whatever their case and accents; one collator for every
 *  comparison, since making one is slow. */
const titles = new Intl.Collator(undefined, { sensitivity: "base" });
const oldest = (a: WorkItem, b: WorkItem) => a.repo.localeCompare(b.repo) || a.number - b.number;
const newest = (a: WorkItem, b: WorkItem) => b.created_at.localeCompare(a.created_at) || -oldest(a, b);

/** Each item's standing, or the best standing of an item under it among
 *  `items` if that is better. */
function treeStandings(items: WorkItem[]) {
  const best = new Map(items.map((i) => [i.key, standing(i)]));
  const parents = new Map(items.map((i) => [i.key, i.parent]));
  for (const item of items) {
    const own = standing(item);
    const seen = new Set([item.key]);
    for (let up = item.parent; up && best.has(up) && !seen.has(up); up = parents.get(up) ?? null) {
      seen.add(up);
      if (best.get(up)! > own) best.set(up, own);
    }
  }
  return best;
}

/** The items in `ordering`. Nested, an item goes by status as the most
 *  active item under it does, so a parent with work in progress comes
 *  before one with none. */
export function orderItems(items: WorkItem[], ordering: Ordering, { nested = false } = {}): WorkItem[] {
  const tree = nested && ordering === "status" ? treeStandings(items) : null;
  const rank = (i: WorkItem) => tree?.get(i.key) ?? standing(i);
  const by: Record<Ordering, (a: WorkItem, b: WorkItem) => number> = {
    oldest,
    newest,
    updated: (a, b) => b.updated_at.localeCompare(a.updated_at) || newest(a, b),
    status: (a, b) => rank(a) - rank(b) || standing(a) - standing(b) || oldest(a, b),
    title: (a, b) => titles.compare(a.title, b.title) || oldest(a, b),
  };
  return [...items].sort(by[ordering]);
}
