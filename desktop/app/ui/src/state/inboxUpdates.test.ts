import { describe, expect, test } from "vitest";
import type { Notification } from "../api/types";
import { lineOf, plain, reasonOf } from "./inboxUpdates";

const note = (id: number, last_event: number, extra: Partial<Notification> = {}) =>
  ({
    id,
    last_event,
    subject: { kind: "item", item: { id: `w_${id}`, key: `PIN-${id}`, title: "Work", status: "open" } },
    repo: "acme/api",
    project: null,
    reason: "item_commented",
    waiting: false,
    actor: { kind: "agent", name: "claude-code", session: null },
    summary: null,
    unread: 1,
    read: false,
    done: false,
    muted: false,
    updated_at: "2026-11-21T10:00:00Z",
    ...extra,
  }) as Notification;

describe("reasonOf", () => {
  test("says who did what", () => {
    expect(reasonOf(note(1, 1))).toBe("claude-code commented");
    expect(reasonOf(note(1, 1, { reason: "item_added", summary: "PIN-4 Apply" }))).toBe("claude-code added a sub-item");
    expect(reasonOf(note(1, 1, { reason: "item_closed", actor: { kind: "system" } }))).toBe("Pinrail marked this done");
  });
});

describe("lineOf", () => {
  const fresh = (extra: object) => ({
    sub_items: 0,
    sub_item: null,
    breakdown: false,
    comments: 0,
    authors: [],
    comment: null,
    changes: 0,
    latest: null,
    parent: null,
    turns: 0,
    reply: null,
    ...extra,
  });
  test("a breakdown, and one or several sub-items", () => {
    expect(
      lineOf(note(1, 1, { reason: "item_added", new: fresh({ latest: "sub_item", breakdown: true, sub_items: 10 }) }))
        .text,
    ).toBe("claude-code added this work item with 10 sub-items");
    expect(
      lineOf(
        note(1, 1, {
          reason: "item_added",
          new: fresh({ latest: "sub_item", sub_items: 1, sub_item: { key: "PIN-708", title: "Show it" } }),
        }),
      ),
    ).toMatchObject({ text: "claude-code added a sub-item", detail: "PIN-708 Show it" });
    expect(lineOf(note(1, 1, { reason: "item_added", new: fresh({ latest: "sub_item", sub_items: 2 }) })).text).toBe(
      "claude-code added 2 sub-items",
    );
  });
  test("comments, and the other changes counted", () => {
    expect(
      lineOf(
        note(1, 1, { new: fresh({ latest: "comment", comments: 3, authors: ["claude-code", "codex"], changes: 2 }) }),
      ),
    ).toEqual({ text: "3 new comments", detail: "claude-code, codex", more: 2, under: undefined });
  });
  test("a finished agent says its last reply, and a sub-item its parent", () => {
    const session = note(1, 1, {
      reason: "agent_finished",
      subject: {
        kind: "session",
        session: { agent: "claude-code", id: "ses_1" },
      } as unknown as Notification["subject"],
      new: fresh({ turns: 3, reply: "Pushed the fix." }),
    });
    expect(lineOf(session)).toEqual({ text: "The agent finished 3 turns", detail: "Pushed the fix." });
    expect(
      lineOf(note(1, 1, { new: fresh({ latest: "change", changes: 1, parent: "PIN-301" }), reason: "item_closed" }))
        .under,
    ).toBe("PIN-301");
  });
});

test("plain takes the Markdown marks out of a line", () => {
  expect(plain("I filed **PIN-711** under `PIN-54`: see [the spec](http://x).")).toBe(
    "I filed PIN-711 under PIN-54: see the spec.",
  );
});
