import { describe, expect, test } from "vitest";
import { KEPT, lastView, openingView, rememberView } from "./sessionView";

function memory() {
  const data = new Map<string, string>();
  return { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
}

describe("session views", () => {
  test("remembers the last view of each session", () => {
    const store = memory();
    rememberView("claude-code/a", "terminal", store);
    rememberView("claude-code/b", "summary", store);
    rememberView("claude-code/a", "summary", store);
    expect(lastView("claude-code/a", store)).toBe("summary");
    expect(lastView("claude-code/b", store)).toBe("summary");
    expect(lastView("claude-code/c", store)).toBeNull();
  });

  test(`keeps the last ${KEPT} sessions`, () => {
    const store = memory();
    for (let i = 0; i <= KEPT; i++) rememberView(`s/${i}`, "terminal", store);
    expect(lastView("s/0", store)).toBeNull();
    expect(lastView("s/1", store)).toBe("terminal");
    expect(lastView(`s/${KEPT}`, store)).toBe("terminal");
  });

  test("reads nothing from storage it cannot parse", () => {
    const store = memory();
    store.setItem("pinrail:session-views", "{not json");
    expect(lastView("s/1", store)).toBeNull();
  });

  test("opens in the view the address names, then the setting, and the summary without a terminal", () => {
    expect(openingView("terminal", "summary", null, true)).toBe("terminal");
    expect(openingView(null, "terminal", "summary", true)).toBe("terminal");
    expect(openingView(null, "last", "terminal", true)).toBe("terminal");
    expect(openingView(null, "last", null, true)).toBe("summary");
    expect(openingView("terminal", "terminal", "terminal", false)).toBe("summary");
  });
});
