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

const oldest = (a: WorkItem, b: WorkItem) => a.repo.localeCompare(b.repo) || a.number - b.number;
const newest = (a: WorkItem, b: WorkItem) => b.created_at.localeCompare(a.created_at) || -oldest(a, b);

/** The items in `ordering`. */
export function orderItems(items: WorkItem[], ordering: Ordering): WorkItem[] {
  const by: Record<Ordering, (a: WorkItem, b: WorkItem) => number> = {
    oldest,
    newest,
    updated: (a, b) => b.updated_at.localeCompare(a.updated_at) || newest(a, b),
    status: (a, b) => standing(a) - standing(b) || oldest(a, b),
    title: (a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: "base" }) || oldest(a, b),
  };
  return [...items].sort(by[ordering]);
}
