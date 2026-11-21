import { describe, expect, test } from "vitest";
import type { Notification } from "../api/types";
import { applyNotification, reasonOf } from "./inboxUpdates";

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

describe("applyNotification", () => {
  test("puts a changed notification first, in place of its old row", () => {
    const list = [note(1, 20), note(2, 10)];
    const next = applyNotification(list, note(2, 30, { unread: 2 }));
    expect(next.map((n) => [n.id, n.unread])).toEqual([
      [2, 2],
      [1, 1],
    ]);
  });
  test("adds a new one, and takes out one that is done", () => {
    const list = applyNotification([note(1, 20)], note(3, 40));
    expect(list.map((n) => n.id)).toEqual([3, 1]);
    expect(applyNotification(list, note(3, 40, { done: true })).map((n) => n.id)).toEqual([1]);
  });
});

describe("reasonOf", () => {
  test("says who did what", () => {
    expect(reasonOf(note(1, 1))).toBe("claude-code commented");
    expect(reasonOf(note(1, 1, { reason: "item_added", summary: "PIN-4 Apply" }))).toBe("claude-code added a sub-item");
    expect(reasonOf(note(1, 1, { reason: "item_closed", actor: { kind: "system" } }))).toBe("Pinrail marked this done");
  });
});
