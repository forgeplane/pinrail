import { describe, expect, test, vi } from "vitest";
import { ApiError, errorMessage, subscribe } from "./client";
import eventsSource from "../../../../core/src/events.rs?raw";

/** Stands in for the browser's EventSource: records the listeners the client adds. */
class FakeSource {
  static last: FakeSource;
  listeners = new Map<string, (event: MessageEvent) => void>();
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  constructor() {
    FakeSource.last = this;
  }
  addEventListener(kind: string, listener: (event: MessageEvent) => void) {
    this.listeners.set(kind, listener);
  }
  close() {}
}

/** Every kind of event the core publishes, read from its source. */
const coreKinds = () => Array.from(eventsSource.matchAll(/pub const \w+: &str = "(\w+)";/g), (m) => m[1]);

describe("subscribe", () => {
  test("hands on every kind of event the core publishes", async () => {
    vi.stubGlobal("EventSource", FakeSource);
    const seen: string[] = [];
    subscribe({ onNotice: (notice) => seen.push(notice.kind) });
    await vi.waitFor(() => expect(FakeSource.last).toBeDefined());
    const kinds = coreKinds();
    expect(kinds).toContain("created");
    for (const kind of kinds) {
      FakeSource.last.listeners.get(kind)?.({ data: JSON.stringify({ kind }) } as MessageEvent);
    }
    expect(seen).toEqual(kinds);
    vi.unstubAllGlobals();
  });
});

describe("errorMessage", () => {
  test("is what the core said, its first violation's when it refused a field", () => {
    expect(errorMessage(new ApiError(409, { error: "conflict", message: "PIN-1 is on hold" }))).toBe(
      "PIN-1 is on hold",
    );
    const refused = new ApiError(422, {
      error: "invalid",
      message: "validation failed",
      violations: [{ path: "/title", message: "is empty" }],
    });
    expect(errorMessage(refused)).toBe("is empty");
  });
  test("is the fallback for anything else", () => {
    expect(errorMessage(new TypeError("Failed to fetch"))).toBe("Pinrail could not be reached.");
    expect(errorMessage(null, "The item was not saved.")).toBe("The item was not saved.");
  });
});
