import { describe, expect, test } from "vitest";
import { KEPT, lastView, openingView, rememberView } from "./sessionView";

function memory() {
  const data = new Map<string, string>();
  return { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
}

describe("session views", () => {
  test("remembers the last view of each session", () => {
    const store = memory();
    rememberView("claude-code/ses_a", "terminal", store);
    rememberView("claude-code/ses_b", "summary", store);
    rememberView("claude-code/ses_a", "summary", store);
    expect(lastView("claude-code/ses_a", store)).toBe("summary");
    expect(lastView("claude-code/ses_b", store)).toBe("summary");
    expect(lastView("claude-code/ses_c", store)).toBeNull();
  });

  test(`keeps the last ${KEPT} sessions`, () => {
    const store = memory();
    for (let i = 0; i <= KEPT; i++) rememberView(`s/ses_${i}`, "terminal", store);
    expect(lastView("s/ses_0", store)).toBeNull();
    expect(lastView("s/ses_1", store)).toBe("terminal");
    expect(lastView(`s/ses_${KEPT}`, store)).toBe("terminal");
  });

  test("drops the views of sessions named by their agent's own id", () => {
    const store = memory();
    store.setItem(
      "pinrail:session-views",
      JSON.stringify([
        ["claude-code/3f2a9c1e-old", "terminal"],
        ["claude-code/ses_a", "split"],
      ]),
    );
    expect(lastView("claude-code/3f2a9c1e-old", store)).toBeNull();
    expect(lastView("claude-code/ses_a", store)).toBe("split");
    rememberView("claude-code/ses_b", "summary", store);
    expect(JSON.parse(store.getItem("pinrail:session-views")!)).toEqual([
      ["claude-code/ses_a", "split"],
      ["claude-code/ses_b", "summary"],
    ]);
  });

  test("reads nothing from storage it cannot parse", () => {
    const store = memory();
    store.setItem("pinrail:session-views", "{not json");
    expect(lastView("s/ses_1", store)).toBeNull();
  });

  test("opens in the view the address names, then the setting, and the summary without a terminal", () => {
    expect(openingView("terminal", "summary", null, true)).toBe("terminal");
    expect(openingView(null, "terminal", "summary", true)).toBe("terminal");
    expect(openingView(null, "last", "terminal", true)).toBe("terminal");
    expect(openingView(null, "last", null, true)).toBe("summary");
    expect(openingView("terminal", "terminal", "terminal", false)).toBe("summary");
  });

  test("the split view is named in the address, remembered, and the summary without a terminal", () => {
    const store = memory();
    rememberView("claude-code/ses_a", "split", store);
    expect(lastView("claude-code/ses_a", store)).toBe("split");
    expect(openingView("split", "summary", null, true)).toBe("split");
    expect(openingView(null, "last", "split", true)).toBe("split");
    expect(openingView("split", "summary", null, false)).toBe("summary");
  });
});
