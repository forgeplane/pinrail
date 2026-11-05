import { describe, expect, test } from "vitest";
import { groupKey, grouped, mixedGroups } from "./grouping";

const pinrail = { id: "p_1", name: "Pinrail" };
type Item = { id: string; repo: string | null; project: typeof pinrail | null };
const items: Item[] = [
  { id: "a", repo: "forgeplane/pinrail", project: pinrail },
  { id: "b", repo: "acme/api", project: null },
  { id: "c", repo: "forgeplane/pinrail-infra", project: pinrail },
  { id: "d", repo: null, project: null },
];

describe("a list grouped by project or by repository", () => {
  test("by project, a project's repositories share a group, and the others keep their own", () => {
    const groups = grouped(items, (i) => groupKey("project", i.repo, i.project));
    expect(groups.map(([key, list]) => [key, list.map((i) => i.id)])).toEqual([
      ["project:p_1", ["a", "c"]],
      ["acme/api", ["b"]],
      ["", ["d"]],
    ]);
    expect([...mixedGroups(groups, (i) => i.repo)]).toEqual(["project:p_1"]);
  });

  test("by repository, each repository is a group, projects or not", () => {
    const groups = grouped(items, (i) => groupKey("repository", i.repo, i.project));
    expect(groups.map(([key]) => key)).toEqual(["forgeplane/pinrail", "acme/api", "forgeplane/pinrail-infra", ""]);
    expect(mixedGroups(groups, (i) => i.repo).size).toBe(0);
  });
});
