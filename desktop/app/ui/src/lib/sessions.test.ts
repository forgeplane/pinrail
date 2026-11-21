import { describe, expect, test } from "vitest";
import type {
  Review,
  SessionComment,
  SessionItemEvent,
  SessionMessage,
  SessionStatus,
  SessionTurn,
} from "../api/types";
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

  test("orders what was sent within one second by its microseconds", () => {
    const at = (us: number) => `2026-10-30T10:00:00.${String(us).padStart(6, "0")}Z`;
    const entries = timeline(
      [review("q1", at(2000)), review("r1", at(4000))],
      [status(1, at(1000), "Starting"), status(2, at(3000), "Sent a question"), status(3, at(5000), "Sent a review")],
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

describe("withdrawn reviews", () => {
  const withdrawn = (id: string, at: string, by: string, revises: string | null = null) =>
    ({ ...review(id, at, revises), status: "withdrawn", withdrawn_at: by }) as Review;
  const show = (entries: ReturnType<typeof timeline>) =>
    entries.map((e) =>
      e.kind === "item" || e.kind === "withdrawn"
        ? `${e.kind} ${e.review.id}`
        : e.kind === "statuses"
          ? `step ${e.statuses[0].text}`
          : e.kind,
    );

  test("adds a line where the agent withdrew a review", () => {
    const entries = timeline(
      [withdrawn("r1", "2026-11-02T10:00:00Z", "2026-11-02T10:10:00Z"), review("r2", "2026-11-02T10:05:00Z")],
      [status(1, "2026-11-02T10:06:00Z", "Before"), status(2, "2026-11-02T10:20:00Z", "After")],
    );
    expect(show(entries)).toEqual(["item r1", "item r2", "step Before", "withdrawn r1", "step After"]);
  });

  test("adds no line for a round that a later round replaced", () => {
    const replaced = {
      ...withdrawn("r1", "2026-11-02T10:00:00Z", "2026-11-02T10:05:00Z"),
      withdrawn_reason: "superseded by r2",
    } as Review;
    const entries = timeline([replaced, review("r2", "2026-11-02T10:05:00Z", "r1")], []);
    expect(show(entries)).toEqual(["item r2"]);
  });

  test("keeps the line when the agent sends a new round of a review it withdrew", () => {
    const entries = timeline(
      [withdrawn("r1", "2026-11-02T10:00:00Z", "2026-11-02T10:05:00Z"), review("r2", "2026-11-02T10:06:00Z", "r1")],
      [],
    );
    expect(show(entries)).toEqual(["item r2", "withdrawn r1", "revised"]);
    expect(entries[1]).toMatchObject({ kind: "withdrawn", card: { id: "r2" } });
  });

  test("puts the line before the next round when both have the same second", () => {
    const entries = timeline(
      [withdrawn("r1", "2026-11-02T10:00:00Z", "2026-11-02T10:05:00Z"), review("r2", "2026-11-02T10:05:00Z", "r1")],
      [],
    );
    expect(show(entries)).toEqual(["item r2", "withdrawn r1", "revised"]);
  });

  test("adds no line for questions, which say so themselves", () => {
    const question = { ...withdrawn("q1", "2026-11-02T10:00:00Z", "2026-11-02T10:05:00Z"), kind: "question" } as Review;
    expect(show(timeline([question], []))).toEqual(["item q1"]);
  });
});

describe("messages", () => {
  /** A message sent at `sent`, which the agent received at `delivered`, or
   *  still waits for with `null`. */
  const message = (id: number, sent: string, delivered: string | null = sent): SessionMessage => ({
    id,
    text: `m${id}`,
    sent_by: "pnezis",
    sent_at: sent,
    delivered_at: delivered,
    state: delivered ? "delivered" : "queued",
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

  test("shows a message where the agent received it, among the reviews and the statuses", () => {
    const entries = timeline(
      [review("r1", "2026-10-30T10:02:00Z")],
      [
        status(1, "2026-10-30T10:00:00Z", "Planning"),
        status(2, "2026-10-30T10:01:00Z", "Building"),
        status(3, "2026-10-30T10:03:00Z", "Testing"),
      ],
      // sent before Building, received with Testing; sent and received last
      [message(1, "2026-10-30T10:00:30Z", "2026-10-30T10:03:00Z"), message(2, "2026-10-30T10:04:00Z")],
    );
    // a message between two steps splits them; at the time of the status
    // that handed it over, it follows it
    expect(show(entries)).toEqual([["Planning", "Building"], "r1", ["Testing"], "m1", "m2"]);
  });

  test("leaves out a message that still waits, which the message box shows", () => {
    const entries = timeline(
      [],
      [status(1, "2026-10-30T10:00:00Z", "Reading"), status(2, "2026-10-30T10:01:00Z", "Switching")],
      [message(1, "2026-10-30T10:00:30Z", null), { ...message(2, "2026-10-30T10:00:40Z", null), state: "unconfirmed" }],
    );
    expect(show(entries)).toEqual([["Reading", "Switching"]]);
  });

  test("shows a message typed into an agent that cannot confirm it where it was sent", () => {
    const entries = timeline(
      [review("r1", "2026-10-30T10:02:00Z")],
      [],
      [{ ...message(1, "2026-10-30T10:00:00Z", null), state: "typed" }],
    );
    expect(show(entries)).toEqual(["m1", "r1"]);
  });
});

describe("comments on work items", () => {
  const comment = (id: string, at: string, body: string): SessionComment => ({
    id,
    body,
    author: "claude-code",
    session: "s-1",
    created_at: at,
    edited_at: null,
    files: [],
    item: { id: `w-${id}`, key: "PIN-1", title: "Work" },
  });

  test("go by their time among the reviews and the statuses, and break a group of steps", () => {
    const entries = timeline(
      [review("r1", "2026-10-30T10:05:00Z")],
      [
        status(1, "2026-10-30T10:01:00Z", "Planning"),
        status(2, "2026-10-30T10:03:00Z", "Building"),
        status(3, "2026-10-30T10:06:00Z", "Testing"),
      ],
      [],
      [
        comment("c1", "2026-10-30T10:02:00Z", "Found the cause."),
        comment("c2", "2026-10-30T10:05:30Z", "Fixed."),
        comment("c3", "2026-10-30T10:08:00Z", "Shipped."),
      ],
    );
    expect(
      entries.map((e) =>
        e.kind === "item"
          ? e.review.id
          : e.kind === "statuses"
            ? e.statuses.map((s) => s.text)
            : e.kind === "comment"
              ? e.comment.body
              : e.kind,
      ),
    ).toEqual([["Planning"], "Found the cause.", ["Building"], "r1", "Fixed.", ["Testing"], "Shipped."]);
  });

  test("go between the statuses they were written between, within one second", () => {
    const at = (ms: number) => `2026-10-30T10:00:00.${String(ms * 1000).padStart(6, "0")}Z`;
    const entries = timeline(
      [],
      [status(1, at(100), "Reading"), status(2, at(300), "Testing"), status(3, at(400), "Done reading", "note")],
      [],
      [comment("c1", at(50), "Before any."), comment("c2", at(200), "After reading.")],
    );
    expect(
      entries.map((e) =>
        e.kind === "comment" ? e.comment.body : e.kind === "statuses" ? e.statuses.map((s) => s.text) : e.kind,
      ),
    ).toEqual(["Before any.", ["Reading"], "After reading.", ["Testing"], "note"]);
  });
});

describe("what the agent did to work items", () => {
  const event = (id: number, at: string, kind: string): SessionItemEvent => ({
    type: "event",
    id,
    kind,
    actor: "claude-code",
    session: "s-1",
    at,
    attrs: null,
    item: { id: "w-1", key: "PIN-1", title: "Work" },
  });

  test("shows each change as a line of its own, by its time, which splits a group of steps", () => {
    const entries = timeline(
      [],
      [status(1, "2026-10-30T10:01:00Z", "Planning"), status(2, "2026-10-30T10:03:00Z", "Building")],
      [],
      [],
      [event(1, "2026-10-30T10:02:00Z", "claimed"), event(2, "2026-10-30T10:04:00Z", "status")],
    );
    expect(
      entries.map((e) =>
        e.kind === "itemEvents"
          ? e.events.map((x) => x.kind)
          : e.kind === "statuses"
            ? e.statuses.map((s) => s.text)
            : e.kind,
      ),
    ).toEqual([["Planning"], ["claimed"], ["Building"], ["status"]]);
  });

  test("puts the same change to several items in a row on one line", () => {
    const at = (s: number) => `2026-10-30T10:00:0${s}.000000Z`;
    const entries = timeline(
      [],
      [],
      [],
      [],
      [event(1, at(1), "created"), event(2, at(2), "created"), event(3, at(3), "claimed"), event(4, at(4), "created")],
    );
    expect(entries.map((e) => (e.kind === "itemEvents" ? e.events.map((x) => x.id) : e.kind))).toEqual([
      [1, 2],
      [3],
      [4],
    ]);
  });
});

describe("a message typed into the agent's terminal", () => {
  const typed = (text: string, deliveredAt: string, route: "type" | "queue" = "type") =>
    ({
      id: 1,
      text,
      sent_by: "p",
      sent_at: "2026-11-15T10:00:00.000Z",
      delivered_at: deliveredAt,
      state: "delivered",
      route,
    }) as SessionMessage;
  const turn = (prompt: string, startedAt: string) =>
    ({
      turn: 1,
      agent_turn: null,
      started_at: startedAt,
      ended_at: null,
      end_kind: null,
      prompt,
      prompt_cut: false,
      reply: "Done.",
      reply_cut: false,
    }) as SessionTurn;
  const prompts = (entries: ReturnType<typeof timeline>) =>
    entries.flatMap((e) => (e.kind === "turn" && e.part === "prompt" ? [e.turn.prompt] : []));

  test("shows once, as the message, and not again as its turn's prompt", () => {
    const entries = timeline(
      [],
      [],
      [typed("Use the v2 endpoint.", "2026-11-15T10:00:01.200Z")],
      [],
      [],
      [turn("Use the v2 endpoint.\n", "2026-11-15T10:00:01.000Z")],
    );
    expect(prompts(entries)).toEqual([]);
    // the reply stays
    expect(entries.find((e) => e.kind === "turn")).toMatchObject({ part: "end", turn: { reply: "Done." } });
  });

  test("leaves the prompt of a turn that it did not start", () => {
    const later = timeline(
      [],
      [],
      [typed("Use the v2 endpoint.", "2026-11-15T10:00:01.200Z")],
      [],
      [],
      [turn("Use the v2 endpoint.", "2026-11-15T10:05:00.000Z")],
    );
    expect(prompts(later)).toEqual(["Use the v2 endpoint."]);
    const queued = timeline(
      [],
      [],
      [typed("Use the v2 endpoint.", "2026-11-15T10:00:01.200Z", "queue")],
      [],
      [],
      [turn("Use the v2 endpoint.", "2026-11-15T10:00:01.000Z")],
    );
    expect(prompts(queued)).toEqual(["Use the v2 endpoint."]);
  });
});

describe("an agent's turn", () => {
  const status = (id: number, text: string, at: string) =>
    ({ id, at, kind: "step", text, attachments: [] }) as unknown as SessionStatus;
  const turn = (over: Partial<SessionTurn>) =>
    ({
      turn: 1,
      agent_turn: null,
      started_at: "2026-11-15T10:00:00.000Z",
      ended_at: null,
      end_kind: null,
      prompt: "Fix it",
      prompt_cut: false,
      reply: null,
      reply_cut: false,
      ...over,
    }) as SessionTurn;
  const kinds = (entries: ReturnType<typeof timeline>) =>
    entries.map((e) => (e.kind === "turn" ? `turn ${e.part}` : e.kind));

  test("shows its prompt where it started and its reply where it ended, after what it did", () => {
    const entries = timeline(
      [],
      [status(1, "Working on PIN-406", "2026-11-15T10:00:30.000Z")],
      [],
      [],
      [],
      [turn({ ended_at: "2026-11-15T10:02:00.000Z", end_kind: "done", reply: "Done." })],
    );
    expect(kinds(entries)).toEqual(["turn prompt", "statuses", "turn end"]);
  });

  test("that went on after it finished shows each reply where it came", () => {
    const entries = timeline(
      [],
      [status(1, "Retrying the item", "2026-11-15T10:03:00.000Z")],
      [],
      [],
      [],
      [
        turn({
          ended_at: "2026-11-15T10:05:00.000Z",
          end_kind: "done",
          reply: "PM-2 is now marked done.",
          replies: [
            { at: "2026-11-15T10:02:00.000Z", text: "Marking PM-2 done has not gone through yet.", cut: false },
            { at: "2026-11-15T10:05:00.000Z", text: "PM-2 is now marked done.", cut: false },
          ],
        }),
      ],
    );
    // the first reply before what came after it, the second where the turn ended
    expect(kinds(entries)).toEqual(["turn prompt", "turnReply", "statuses", "turn end"]);
  });

  test("shows nothing while it runs with its prompt shown as a message", () => {
    expect(timeline([], [], [], [], [], [turn({ prompt: null })])).toEqual([]);
  });
});

describe("a turn that ended without a reply", () => {
  const turn = (n: number, startedAt: string, endedAt: string | null, endKind: string | null) =>
    ({
      turn: n,
      agent_turn: null,
      started_at: startedAt,
      ended_at: endedAt,
      end_kind: endKind,
      prompt: `Prompt ${n}`,
      prompt_cut: false,
      reply: null,
      reply_cut: false,
    }) as SessionTurn;
  const ends = (entries: ReturnType<typeof timeline>) =>
    entries.flatMap((e) => (e.kind === "turn" && e.part === "end" ? [e.turn.turn] : []));

  test("shows no end when the next prompt closed it, as a message sent while the agent works does", () => {
    const entries = timeline(
      [],
      [],
      [],
      [],
      [],
      [
        turn(1, "2026-11-20T09:44:00.000Z", "2026-11-20T09:44:03.000Z", "interrupted"),
        turn(2, "2026-11-20T09:44:03.000Z", null, null),
      ],
    );
    expect(ends(entries)).toEqual([]);
  });

  test("shows its end when the agent reported the interrupt", () => {
    const entries = timeline(
      [],
      [],
      [],
      [],
      [],
      [
        turn(1, "2026-11-20T09:22:00.000Z", "2026-11-20T09:31:50.000Z", "interrupted"),
        turn(2, "2026-11-20T09:40:00.000Z", null, null),
      ],
    );
    expect(ends(entries)).toEqual([1]);
  });
});
