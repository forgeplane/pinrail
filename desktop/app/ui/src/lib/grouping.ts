// Lists grouped by project or by repository, as the inbox and the Sessions
// page show them: a project's group holds all its repositories, a
// repository in no project is a group of its own, and the groups come in
// the order of their first item.

import type { ProjectRef } from "../api/types";

/** How a list of reviews or sessions is laid out. */
export type Layout = "project" | "repository" | "list";

/** The group an item goes in: `project:<id>` for a project's, or its repository, "" for none. */
export function groupKey(layout: Layout, repo: string | null | undefined, project: ProjectRef | null | undefined) {
  return layout === "project" && project ? `project:${project.id}` : (repo ?? "");
}

/** The items in groups, each group where its first item fell, in order within it. */
export function grouped<T>(items: T[], keyOf: (item: T) => string): [string, T[]][] {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const key = keyOf(item);
    groups.set(key, [...(groups.get(key) ?? []), item]);
  }
  return [...groups.entries()];
}

/** The groups that hold more than one repository: their rows name their repository. */
export function mixedGroups<T>(groups: [string, T[]][], repoOf: (item: T) => string | null | undefined) {
  return new Set(
    groups
      .filter(([key, items]) => key.startsWith("project:") && new Set(items.map((i) => repoOf(i) ?? "")).size > 1)
      .map(([key]) => key),
  );
}
