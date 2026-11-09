import { describe, expect, test, vi } from "vitest";
import { ApiError, errorMessage, subscribe } from "./client";
import eventsSource from "../../../../core/src/events.rs?raw";

/** Stands in for the browser's EventSource: records the listeners the client adds. */
class FakeSource {
  static last: FakeSource;
  static all: FakeSource[] = [];
  listeners = new Map<string, (event: MessageEvent) => void>();
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  closed = false;
  constructor(readonly url: string) {
    FakeSource.last = this;
    FakeSource.all.push(this);
  }
  addEventListener(kind: string, listener: (event: MessageEvent) => void) {
    this.listeners.set(kind, listener);
  }
  close() {
    this.closed = true;
  }
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

  test("a new connection catches up from the last event seen, and each event arrives once", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("EventSource", FakeSource);
    // the client waits with window.setTimeout, which the fake timers drive
    vi.stubGlobal("window", globalThis);
    FakeSource.all = [];
    const seen: number[] = [];
    const stop = subscribe({ onNotice: (notice) => seen.push(notice.event_id) });
    await vi.waitFor(() => expect(FakeSource.all).toHaveLength(1));
    const send = (source: FakeSource, id: number) =>
      source.listeners.get("created")!({
        data: JSON.stringify({ kind: "created", event_id: id }),
        lastEventId: String(id),
      } as MessageEvent);
    const first = FakeSource.all[0];
    expect(first.url).toMatch(/\/api\/v1\/events$/);
    send(first, 6);
    send(first, 7);

    // the connection drops; the client tries again after a short wait
    first.onerror!();
    expect(first.closed).toBe(true);
    await vi.advanceTimersByTimeAsync(250);
    expect(FakeSource.all).toHaveLength(2);
    const second = FakeSource.all[1];
    expect(second.url).toMatch(/\/api\/v1\/events\?after=7$/);
    // the core replays what came after 7, then goes on
    send(second, 8);
    send(second, 9);
    expect(seen).toEqual([6, 7, 8, 9]);

    stop();
    vi.useRealTimers();
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
