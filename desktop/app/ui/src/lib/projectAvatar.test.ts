import { describe, expect, test } from "vitest";
import { PROJECT_COLORS, colorOf, iconInput, letterSize, markOf } from "./projectAvatar";
import coreProjects from "../../../../core/src/projects.rs?raw";

describe("a project's avatar", () => {
  test("shows the colour the person chose", () => {
    expect(colorOf({ id: "p_1", color: "green" })).toBe("green");
  });

  test("without a colour, or with one the app does not know, takes one by its id, the same each time", () => {
    const picked = colorOf({ id: "p_01ABC" });
    expect(PROJECT_COLORS).toContain(picked);
    expect(colorOf({ id: "p_01ABC", color: null })).toBe(picked);
    expect(colorOf({ id: "p_01ABC", color: "chartreuse" })).toBe(picked);
    // projects spread over the palette rather than sharing one colour
    const colors = new Set(Array.from({ length: 40 }, (_, i) => colorOf({ id: `p_${i}` })));
    expect(colors.size).toBeGreaterThan(5);
  });

  test("shows the emoji, or the name's first letter in capitals", () => {
    expect(markOf({ name: "Pinrail", emoji: "🚂" })).toBe("🚂");
    expect(markOf({ name: "wicket" })).toBe("W");
    expect(markOf({ name: "  crier", emoji: "" })).toBe("C");
    // a letter made of two characters stays whole
    expect(markOf({ name: "🦀 Rust" })).toBe("🦀");
    expect(markOf({ name: "" })).toBe("?");
    // letters given as the icon show in capitals
    expect(markOf({ name: "Pinrail", emoji: "api" })).toBe("API");
  });

  test("shrinks its letters as there are more of them, so that three fit", () => {
    expect(letterSize("P")).toBeGreaterThan(letterSize("PR"));
    expect(letterSize("PR")).toBeGreaterThan(letterSize("API"));
    // three letters, each about as wide as it is tall, fit in the square
    expect(letterSize("API") * 3).toBeLessThan(1.05);
  });

  test("the icon field keeps up to three letters, and an emoji whole", () => {
    expect(iconInput("pinrail")).toBe("pin");
    expect(iconInput("ab")).toBe("ab");
    expect(iconInput("👩🏽‍💻")).toBe("👩🏽‍💻");
    expect(iconInput("")).toBe("");
  });
});

describe("the palette", () => {
  test("has the sixteen colours the core accepts", () => {
    const core = coreProjects.match(/pub const COLORS: \[&str; \d+\] = \[([^\]]+)\]/)![1];
    const accepted = [...core.matchAll(/"(\w+)"/g)].map((m) => m[1]);
    expect(PROJECT_COLORS).toHaveLength(16);
    expect([...PROJECT_COLORS].sort()).toEqual([...accepted].sort());
  });
});
