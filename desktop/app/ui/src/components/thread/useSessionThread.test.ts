import { describe, expect, test } from "vitest";
import type { Notice, Review } from "../../api/types";
import { aboutThread } from "./useSessionThread";

const named = { agent: "claude-code", id: "s-1" };
const reviews = new Set(["r_1"]);
const items = new Set(["PIN-7"]);
const notice = (extra: Partial<Notice>): Notice => ({
  event_id: 1,
  kind: "review_updated",
  review_id: null,
  review: null,
  ...extra,
});
const review = (requested_by: string, session: string) => ({ requested_by, session }) as Review;
const item = (key: string, agent: string | null, session: string | null) => ({ id: `w_${key}`, key, agent, session });

describe("aboutThread", () => {
  test("loads the thread again for its own reviews, statuses and messages", () => {
    for (const ours of [
      notice({ review_id: "r_1" }),
      notice({ review_id: "r_9", review: review("claude-code", "s-1") }),
      notice({ kind: "session_status", session: { agent: "claude-code", id: "s-1" } as Notice["session"] }),
    ])
      expect(aboutThread(ours, named, reviews, items)).toEqual({ thread: true, item: false });
  });

  test("loads its items again for a change its agent made, or for one of its items", () => {
    for (const ours of [
      notice({ kind: "item_changed", item: item("PIN-9", "claude-code", "s-1") }),
      notice({ kind: "item_changed", item: item("PIN-7", null, null) }),
    ])
      expect(aboutThread(ours, named, reviews, items)).toEqual({ thread: true, item: true });
  });

  test("leaves the thread alone for other sessions", () => {
    for (const theirs of [
      notice({ review_id: "r_9", review: review("claude-code", "s-2") }),
      notice({ review_id: "r_9", review: review("codex", "s-1") }),
      notice({ kind: "session_status", session: { agent: "claude-code", id: "s-2" } as Notice["session"] }),
      notice({ kind: "item_changed", item: item("PIN-9", "claude-code", "s-2") }),
      notice({ kind: "settings_changed" }),
    ])
      expect(aboutThread(theirs, named, reviews, items)).toEqual({ thread: false, item: false });
  });

  test("without a session, only the review it shows counts", () => {
    expect(aboutThread(notice({ review_id: "r_1" }), null, reviews, items).thread).toBe(true);
    expect(
      aboutThread(notice({ kind: "item_changed", item: item("PIN-7", null, null) }), null, reviews, items),
    ).toEqual({ thread: false, item: false });
  });
});
