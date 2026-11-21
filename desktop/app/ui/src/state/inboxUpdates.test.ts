import { describe, expect, test } from "vitest";
import type { Notification } from "../api/types";
import type { Notice } from "../api/types";
import { eventLine, reasonOf } from "./inboxUpdates";

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

describe("eventLine", () => {
  const notice = (kind: string, extra: object) => ({ event_id: 1, kind, ...extra }) as unknown as Notice;
  test("says what changed on a work item, with its detail", () => {
    expect(eventLine(notice("item_changed", { item: { change: "commented", excerpt: "Ready." } }))).toEqual({
      text: "commented",
      detail: "Ready.",
    });
    expect(eventLine(notice("item_changed", { item: { change: "status", to: "in_progress" } }))?.text).toBe(
      "moved it to In progress",
    );
  });
  test("leaves out an agent at work", () => {
    expect(eventLine(notice("session_presence", { session: { presence: { state: "working" } } }))).toBeNull();
    expect(
      eventLine(
        notice("session_presence", {
          session: { presence: { state: "needs_input", wait: { kind: "permission", summary: "cargo test" } } },
        }),
      ),
    ).toEqual({ text: "asked for permission", detail: "cargo test" });
  });
});
