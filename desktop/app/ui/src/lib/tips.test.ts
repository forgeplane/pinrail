import { describe, expect, test } from "vitest";
import { TIPS, tipParts } from "./tips";

describe("tipParts", () => {
  test("splits a tip into text and the keys of each shortcut", () => {
    expect(tipParts("{Ctrl+`} switches views, and {[} and {]} move")).toEqual([
      ["Ctrl", "`"],
      " switches views, and ",
      ["["],
      " and ",
      ["]"],
      " move",
    ]);
  });

  test("names the modifiers as the platform does", () => {
    const [keys] = tipParts("{MOD+SHIFT+X}");
    expect(keys).toHaveLength(3);
    expect(keys).not.toContain("MOD");
    expect(keys).not.toContain("SHIFT");
  });
});

test("every tip has a category and closes its braces", () => {
  for (const tip of TIPS) {
    expect(tip.category).not.toBe("");
    expect(tip.text.split("{").length, tip.text).toBe(tip.text.split("}").length);
  }
});
