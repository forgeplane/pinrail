import { describe, expect, test } from "vitest";
import type { Review, SessionStatus } from "../api/types";
import { tasks, timeline } from "./sessions";

const review = (id: string, at: string) => ({ id, created_at: at }) as Review;
const status = (id: number, at: string, text: string, kind: SessionStatus["kind"] = "step"): SessionStatus => ({
  id,
  at,
  text,
  kind,
});

describe("timeline", () => {
  test("puts the statuses between the reviews, in order, grouped when in a row", () => {
    const entries = timeline(
      [review("r1", "2026-10-30T10:00:10Z"), review("r2", "2026-10-30T10:05:00Z")],
      [
        status(1, "2026-10-30T10:00:00Z", "Add rate limiting", "start"),
        status(2, "2026-10-30T10:01:00Z", "Planning"),
        status(3, "2026-10-30T10:02:00Z", "Building"),
        status(4, "2026-10-30T10:06:00Z", "", "done"),
        status(5, "2026-10-30T10:07:00Z", "Fixing a comment"),
      ],
    );
    expect(
      entries.map((e) =>
        e.kind === "item" ? e.review.id : e.kind === "statuses" ? e.statuses.map((s) => s.text) : e.kind,
      ),
    ).toEqual(["started", "r1", ["Planning", "Building"], "r2", "finished", ["Fixing a comment"]]);
  });

  test("puts a status sent in the same second as a review after it", () => {
    const entries = timeline([review("r1", "2026-10-30T10:00:00Z")], [status(1, "2026-10-30T10:00:00Z", "Waiting")]);
    expect(entries.map((e) => e.kind)).toEqual(["item", "statuses"]);
  });

  test("puts a status after the review the core says it came after", () => {
    const at = "2026-10-30T10:00:00Z";
    const entries = timeline(
      [review("q1", at), review("r1", at)],
      [
        { ...status(1, at, "Starting"), after: null },
        { ...status(2, at, "Sent a question"), after: "q1" },
        { ...status(3, at, "Sent a review"), after: "r1" },
      ],
    );
    expect(
      entries.map((e) => (e.kind === "item" ? e.review.id : e.kind === "statuses" ? e.statuses[0].text : e.kind)),
    ).toEqual(["Starting", "q1", "Sent a question", "r1", "Sent a review"]);
  });
});

describe("tasks", () => {
  test("gives each task its end, and marks the one in progress", () => {
    const all = tasks([
      status(1, "2026-10-30T10:00:00Z", "Add rate limiting", "start"),
      status(2, "2026-10-30T10:05:00Z", "Testing"),
      status(3, "2026-10-30T10:20:00Z", "Opened #214", "done"),
      status(4, "2026-10-30T10:30:00Z", "Fix the review comments", "start"),
      status(5, "2026-10-30T10:40:00Z", "Document the limits", "start"),
      status(6, "2026-10-30T10:45:00Z", "Writing"),
    ]);
    expect(all.map((t) => [t.start.text, t.done?.text ?? null, t.current])).toEqual([
      ["Add rate limiting", "Opened #214", false],
      ["Fix the review comments", null, false],
      ["Document the limits", null, true],
    ]);
  });

  test("has none in progress once the agent finished", () => {
    const all = tasks([
      status(1, "2026-10-30T10:00:00Z", "Add rate limiting", "start"),
      status(2, "2026-10-30T10:01:00Z", "", "done"),
    ]);
    expect(all.map((t) => t.current)).toEqual([false]);
  });
});
