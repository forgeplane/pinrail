import { describe, expect, test } from "vitest";
import type { Review, SessionMessage, SessionStatus } from "../api/types";
import { newestRounds, stageReviews, stages, timeline } from "./sessions";

const review = (id: string, at: string, revises: string | null = null) =>
  ({ id, created_at: at, kind: "review", revises }) as Review;
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

  test("shows a note on its own, never in a group of steps", () => {
    const entries = timeline(
      [],
      [
        status(1, "2026-10-30T10:01:00Z", "Planning"),
        status(2, "2026-10-30T10:02:00Z", "## Found\n\n- three callers", "note"),
        status(3, "2026-10-30T10:03:00Z", "Building"),
      ],
    );
    expect(entries.map((e) => (e.kind === "statuses" ? e.statuses.map((s) => s.text) : e.kind))).toEqual([
      ["Planning"],
      "note",
      ["Building"],
    ]);
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

describe("stages", () => {
  test("gives each stage its end, and marks the one in progress", () => {
    const all = stages([
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
    const all = stages([
      status(1, "2026-10-30T10:00:00Z", "Add rate limiting", "start"),
      status(2, "2026-10-30T10:01:00Z", "", "done"),
    ]);
    expect(all.map((t) => t.current)).toEqual([false]);
  });
});

describe("stageReviews", () => {
  test("gives each stage the reviews after its start, and leaves out questions", () => {
    const question = { ...review("q1", "2026-10-30T10:02:00Z"), kind: "question" } as Review;
    const entries = timeline(
      [
        review("before", "2026-10-30T09:00:00Z"),
        review("r1", "2026-10-30T10:01:00Z"),
        question,
        review("r2", "2026-10-30T11:01:00Z"),
      ],
      [status(1, "2026-10-30T10:00:00Z", "First", "start"), status(2, "2026-10-30T11:00:00Z", "Second", "start")],
    );
    const by = stageReviews(entries);
    expect(by.get(1)?.map((r) => r.id)).toEqual(["r1"]);
    expect(by.get(2)?.map((r) => r.id)).toEqual(["r2"]);
  });
});

describe("rounds", () => {
  const show = (entries: ReturnType<typeof timeline>) =>
    entries.map((e) =>
      e.kind === "item"
        ? `${e.review.id}${e.round ? ` ${e.round.n}/${e.round.of}` : ""}`
        : e.kind === "revised"
          ? `revised ${e.review.id} ${e.round} of ${e.card.id}`
          : e.kind,
    );

  test("keeps a review revised in a later stage in its stage, with a line where the round was sent", () => {
    const entries = timeline(
      [review("r1", "2026-10-30T10:01:00Z"), review("r2", "2026-10-30T10:06:00Z", "r1")],
      [status(1, "2026-10-30T10:00:00Z", "Spec", "start"), status(2, "2026-10-30T10:05:00Z", "Docs", "start")],
    );
    expect(show(entries)).toEqual(["started", "r2 2/2", "started", "revised r2 2 of r2"]);
    // the stage that sent the first round keeps it, by its newest round
    expect(
      stageReviews(entries)
        .get(1)!
        .map((r) => r.id),
    ).toEqual(["r2"]);
    expect(stageReviews(entries).get(2)).toEqual([]);
  });

  test("adds no line for a round that follows its review with only statuses between", () => {
    const entries = timeline(
      [review("r1", "2026-10-30T10:01:00Z"), review("r2", "2026-10-30T10:03:00Z", "r1")],
      [status(1, "2026-10-30T10:02:00Z", "Revising")],
    );
    expect(show(entries)).toEqual(["r2 2/2", "statuses"]);
  });

  test("adds a line for each later round sent after something else", () => {
    const entries = timeline(
      [
        review("r1", "2026-10-30T10:01:00Z"),
        review("o", "2026-10-30T10:02:00Z"),
        review("r2", "2026-10-30T10:03:00Z", "r1"),
        review("r3", "2026-10-30T10:04:00Z", "r2"),
        review("p", "2026-10-30T10:05:00Z"),
        review("r4", "2026-10-30T10:06:00Z", "r3"),
      ],
      [],
    );
    expect(show(entries)).toEqual(["r4 4/4", "o", "revised r2 2 of r4", "p", "revised r4 4 of r4"]);
  });

  test("shows a round of a review from elsewhere as a card without a round", () => {
    const entries = timeline([review("r2", "2026-10-30T10:01:00Z", "gone")], []);
    expect(show(entries)).toEqual(["r2"]);
  });

  test("counts each review once, by its newest round", () => {
    const items = [
      review("r1", "2026-10-30T10:01:00Z"),
      review("o", "2026-10-30T10:02:00Z"),
      review("r2", "2026-10-30T10:03:00Z", "r1"),
    ];
    expect(newestRounds(items).map((r) => r.id)).toEqual(["o", "r2"]);
  });
});

describe("messages", () => {
  const message = (id: number, at: string, after: number | null = null): SessionMessage => ({
    id,
    text: `m${id}`,
    sent_by: "pnezis",
    sent_at: at,
    delivered_at: null,
    after_status: after,
  });
  const show = (entries: ReturnType<typeof timeline>) =>
    entries.map((e) =>
      e.kind === "item"
        ? e.review.id
        : e.kind === "statuses"
          ? e.statuses.map((s) => s.text)
          : e.kind === "message"
            ? `m${e.message.id}`
            : e.kind,
    );

  test("shows a message when it was sent, among the reviews and the statuses", () => {
    const entries = timeline(
      [review("r1", "2026-10-30T10:02:00Z")],
      [
        status(1, "2026-10-30T10:00:00Z", "Planning"),
        status(2, "2026-10-30T10:01:00Z", "Building"),
        status(3, "2026-10-30T10:03:00Z", "Testing"),
      ],
      [message(1, "2026-10-30T10:00:30Z", 1), message(2, "2026-10-30T10:04:00Z", 3)],
    );
    // a message between two steps splits them
    expect(show(entries)).toEqual([["Planning"], "m1", ["Building"], "r1", ["Testing"], "m2"]);
  });

  test("puts a message after the status that was the newest, though they share a second", () => {
    const entries = timeline(
      [],
      [status(1, "2026-10-30T10:00:00Z", "Reading"), status(2, "2026-10-30T10:00:00Z", "Switching")],
      [message(1, "2026-10-30T10:00:00Z", 1)],
    );
    expect(show(entries)).toEqual([["Reading"], "m1", ["Switching"]]);
  });

  test("shows a message sent before anything else first", () => {
    const entries = timeline([review("r1", "2026-10-30T10:02:00Z")], [], [message(1, "2026-10-30T10:00:00Z")]);
    expect(show(entries)).toEqual(["m1", "r1"]);
  });
});
