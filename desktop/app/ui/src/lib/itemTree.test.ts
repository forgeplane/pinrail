import { describe, expect, test } from "vitest";
import type { WorkItem } from "../api/types";
import { itemTree } from "./itemTree";

const item = (key: string, parent: string | null = null): WorkItem => ({ key, parent }) as WorkItem;
const shape = (rows: ReturnType<typeof itemTree>) => rows.map((r) => `${"  ".repeat(r.depth)}${r.item.key}`);

describe("itemTree", () => {
  // A-1 has A-2 and A-4 under it, and A-3 under A-2
  const list = [item("A-1"), item("A-2", "A-1"), item("A-5"), item("A-3", "A-2"), item("A-4", "A-1")];

  test("sub-items nest under their parent, in the list's order", () => {
    expect(shape(itemTree(list, { nested: true, folded: new Set() }))).toEqual([
      "A-1",
      "  A-2",
      "    A-3",
      "  A-4",
      "A-5",
    ]);
  });

  test("a sub-item whose parent is not in the list stays at the top level", () => {
    const rows = itemTree([item("A-2", "A-1"), item("A-3", "A-2")], { nested: true, folded: new Set() });
    expect(shape(rows)).toEqual(["A-2", "  A-3"]);
  });

  test("a folded parent hides everything under it", () => {
    const rows = itemTree(list, { nested: true, folded: new Set(["A-1"]) });
    expect(shape(rows)).toEqual(["A-1", "A-5"]);
    expect(rows[0].folded).toBe(true);
  });

  test("flat, every item is a row at the top level", () => {
    const rows = itemTree(list, { nested: false, folded: new Set(["A-1"]) });
    expect(rows.map((r) => r.item.key)).toEqual(["A-1", "A-2", "A-5", "A-3", "A-4"]);
    expect(rows.every((r) => r.depth === 0 && !r.folded && r.children === 0)).toBe(true);
  });

  test("each row counts its own sub-items in the list", () => {
    const rows = itemTree(list, { nested: true, folded: new Set() });
    expect(Object.fromEntries(rows.map((r) => [r.item.key, r.children]))).toEqual({
      "A-1": 2,
      "A-2": 1,
      "A-3": 0,
      "A-4": 0,
      "A-5": 0,
    });
  });
});
