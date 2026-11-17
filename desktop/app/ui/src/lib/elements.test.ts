import { expect, test } from "vitest";
import type { Element } from "../api/types";
import { newlyDiscovered } from "./elements";

const element = (number: number, rarity: Element["rarity"]): Element => ({
  number,
  symbol: `E${number}`,
  name: `Element ${number}`,
  category: "noble gas",
  rarity,
  row: 1,
  column: number,
  description: "One sentence.",
});
const elements = [element(1, "common"), element(2, "rare"), element(3, "legendary")];

test("an element no session had is discovered once, with how many are known then", () => {
  const known = new Set([1]);
  const found = newlyDiscovered(known, elements, [{ element: 1 }, { element: 2 }, { element: null }, { element: 2 }]);
  expect(found.map((f) => [f.element.number, f.count])).toEqual([[2, 2]]);
  // seen again in a later list, it is no longer new
  expect(newlyDiscovered(known, elements, [{ element: 2 }, { element: 3 }]).map((f) => f.element.number)).toEqual([3]);
});
