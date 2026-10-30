import { describe, expect, test } from "vitest";
import type { Review, SessionStatus } from "../api/types";
import { timeline } from "./sessions";

const review = (id: string, at: string) => ({ id, created_at: at }) as Review;
const status = (id: number, at: string, text: string, done = false): SessionStatus => ({ id, at, text, done });

describe("timeline", () => {
  test("puts the statuses between the reviews, in order, grouped when in a row", () => {
    const entries = timeline(
      [review("r1", "2026-10-30T10:00:10Z"), review("r2", "2026-10-30T10:05:00Z")],
      [
        status(1, "2026-10-30T10:00:00Z", "Reading"),
        status(2, "2026-10-30T10:01:00Z", "Planning"),
        status(3, "2026-10-30T10:02:00Z", "Building"),
        status(4, "2026-10-30T10:06:00Z", "", true),
        status(5, "2026-10-30T10:07:00Z", "Fixing a comment"),
      ],
    );
    expect(
      entries.map((e) =>
        e.kind === "item" ? e.review.id : e.kind === "finished" ? "finished" : e.statuses.map((s) => s.text),
      ),
    ).toEqual([["Reading"], "r1", ["Planning", "Building"], "r2", "finished", ["Fixing a comment"]]);
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
