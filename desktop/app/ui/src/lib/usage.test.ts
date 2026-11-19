import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

const countFeature = vi.fn(() => Promise.resolve());
vi.mock("../api/client", () => ({ api: { countFeature } }));

describe("pageFeature", () => {
  test("names the pages that are counted, by their path", async () => {
    const { pageFeature } = await import("./usage");
    expect(pageFeature("/", "")).toBe("page_inbox");
    expect(pageFeature("/", "?tab=history&q=x")).toBe("page_history");
    expect(pageFeature("/sessions", "")).toBe("page_sessions");
    expect(pageFeature("/sessions/claude-code/s-1", "?view=terminal")).toBe("page_sessions");
    expect(pageFeature("/items", "?repo=acme")).toBe("page_items");
    expect(pageFeature("/projects", "")).toBe("page_projects");
  });

  test("leaves out the pages that are not counted", async () => {
    const { pageFeature } = await import("./usage");
    expect(pageFeature("/reviews/r_1", "")).toBeNull();
    expect(pageFeature("/sessions-old", "")).toBeNull();
    expect(pageFeature("/plugins", "")).toBeNull();
  });
});

describe("countFeature", () => {
  beforeEach(() => {
    vi.resetModules();
    countFeature.mockClear();
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  test("reports a feature once a day, however often it is used", async () => {
    const usage = await import("./usage");
    vi.setSystemTime(new Date("2026-11-19T09:00:00"));
    usage.countFeature("command_palette");
    usage.countFeature("command_palette");
    usage.countFeature("quick_add");
    expect(countFeature.mock.calls).toEqual([["command_palette"], ["quick_add"]]);

    // the next day, again
    vi.setSystemTime(new Date("2026-11-20T09:00:00"));
    usage.countFeature("command_palette");
    expect(countFeature.mock.calls).toEqual([["command_palette"], ["quick_add"], ["command_palette"]]);
  });

  test("a report the core refuses does no harm", async () => {
    countFeature.mockImplementationOnce(() => Promise.reject(new Error("off")));
    const usage = await import("./usage");
    expect(() => usage.countFeature("find_in_thread")).not.toThrow();
    await vi.runAllTimersAsync();
  });
});
