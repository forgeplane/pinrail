import { describe, expect, test } from "vitest";
import type { WorkItem } from "../api/types";
import { fuzzyMatch } from "./fuzzy";
import { insertKey, offers, pickerQuery } from "./itemPicker";

describe("fuzzyMatch", () => {
  test("matches the letters in order, and not out of order", () => {
    expect(fuzzyMatch("lgn", "Fix the login")?.indices).toEqual([8, 10, 12]);
    expect(fuzzyMatch("ngl", "Fix the login")).toBeNull();
  });

  test("scores letters together, and at the start of a word, higher", () => {
    const together = fuzzyMatch("log", "Fix the login")!.score;
    const apart = fuzzyMatch("log", "a lot of gear")!.score;
    const inside = fuzzyMatch("gin", "Fix the login")!.score;
    expect(together).toBeGreaterThan(apart);
    expect(together).toBeGreaterThan(inside);
  });
});

describe("pickerQuery", () => {
  test("finds the query after a # that starts a word, before the cursor", () => {
    expect(pickerQuery("see #log", 8)).toEqual({ at: 4, query: "log" });
    expect(pickerQuery("#", 1)).toEqual({ at: 0, query: "" });
    expect(pickerQuery("line\n#PIN-4", 11)).toEqual({ at: 5, query: "PIN-4" });
  });

  test("finds none inside a word, after a space, or in a heading", () => {
    expect(pickerQuery("issue#4", 7)).toBeNull();
    expect(pickerQuery("#log in", 7)).toBeNull();
    expect(pickerQuery("# Title", 7)).toBeNull();
  });
});

const item = (key: string, title: string, repo = "acme/api", updated = "2026-11-01T00:00:00Z") =>
  ({ key, number: Number(key.split("-")[1]), title, repo, updated_at: updated }) as WorkItem;

describe("offers", () => {
  const items = [
    item("PIN-4", "Fix the login", "forgeplane/pinrail"),
    item("API-12", "Log every request"),
    item("API-40", "Login with a passkey"),
    item("API-7", "Rename the table"),
  ];

  test("lists the items of the current repository first, each group by how well they match", () => {
    expect(offers(items, "login", "acme/api").map((o) => o.item.key)).toEqual(["API-40", "PIN-4"]);
    expect(offers(items, "login", "forgeplane/pinrail").map((o) => o.item.key)).toEqual(["PIN-4", "API-40"]);
  });

  test("puts the item whose key or number is typed whole first", () => {
    expect(offers(items, "12", "acme/api")[0].item.key).toBe("API-12");
    expect(offers(items, "pin-4", "acme/api")[0].item.key).toBe("PIN-4");
  });

  test("marks where the key or the title matched", () => {
    const [login] = offers(items, "login", "acme/api");
    expect(login.titleHits).toEqual([0, 1, 2, 3, 4]);
    expect(login.keyHits).toEqual([]);
    expect(offers(items, "api-7", "acme/api")[0].keyHits).toEqual([0, 1, 2, 3, 4]);
  });

  test("lists every item, newest first, before anything is typed", () => {
    const newer = item("API-50", "Newer", "acme/api", "2026-11-09T00:00:00Z");
    expect(offers([...items, newer], "", "acme/api")[0].item.key).toBe("API-50");
  });
});

describe("insertKey", () => {
  test("replaces the # and the query with the key and a space", () => {
    expect(insertKey("see #log", 4, 8, "PIN-4")).toEqual({ text: "see PIN-4 ", cursor: 10 });
  });

  test("adds no space before one already there", () => {
    expect(insertKey("see #lo now", 4, 7, "PIN-4")).toEqual({ text: "see PIN-4 now", cursor: 9 });
  });
});
