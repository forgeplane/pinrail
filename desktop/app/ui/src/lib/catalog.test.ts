import { describe, expect, it } from "vitest";
import type { CatalogEntry } from "../api/types";
import { isPath, matching } from "./catalog";

const entry = (name: string, fields: Partial<CatalogEntry> = {}): CatalogEntry => ({
  id: `forgeplane/${name}`,
  name,
  version: "1.0.0",
  title: name,
  description: null,
  use_when: null,
  icon: null,
  official: true,
  recommended: false,
  installed: null,
  needs: null,
  ...fields,
});

describe("the install field", () => {
  it("takes a path to a folder or a zip, and anything else as words to search for", () => {
    for (const path of ["/abs/review", "~/code/review", "./review", "../review", "C:\\plugins\\review", "review.zip"]) {
      expect(isPath(path), path).toBe(true);
    }
    for (const words of ["list", "code review", "forgeplane/list", ""]) {
      expect(isPath(words), words).toBe(false);
    }
  });

  it("finds official plugins by name, title or description, and lists those not installed when empty", () => {
    const entries = [
      entry("list", { title: "Action list", installed: "1.0.0" }),
      entry("feedback", { title: "Feedback", description: "Questions answered in one pass" }),
    ];
    expect(matching(entries, "").map((e) => e.name)).toEqual(["feedback"]);
    expect(matching(entries, "ACTION").map((e) => e.name)).toEqual(["list"]);
    expect(matching(entries, "questions").map((e) => e.name)).toEqual(["feedback"]);
    expect(matching(entries, "calendar")).toEqual([]);
  });
});
