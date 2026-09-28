import { describe, expect, test } from "vitest";
import type { Notice, Review } from "../api/types";
import { applyNotice } from "./pending";

const review = (id: string, status = "pending", title = id) => ({ id, status, title }) as unknown as Review;
const notice = (kind: string, about: Review | null, id = about?.id ?? null): Notice => ({
  event_id: 1,
  kind,
  review_id: id,
  review: about,
});

// the list the server gives: newest first
const listed = [review("r_3"), review("r_1")];

describe("applyNotice", () => {
  test("adds a new review in its place, newest first", () => {
    const next = applyNotice(listed, notice("created", review("r_2")));
    expect(next.map((r) => r.id)).toEqual(["r_3", "r_2", "r_1"]);
    expect(applyNotice(listed, notice("created", review("r_4"))).map((r) => r.id)).toEqual(["r_4", "r_3", "r_1"]);
  });

  test("removes a review that ends, however it ends", () => {
    for (const kind of ["decided", "withdrawn", "discarded", "expired"]) {
      expect(
        applyNotice(listed, notice(kind, review("r_3", kind))).map((r) => r.id),
        kind,
      ).toEqual(["r_1"]);
    }
  });

  test("removes a listed review the event shows is no longer pending", () => {
    expect(applyNotice(listed, notice("viewed", review("r_1", "decided"))).map((r) => r.id)).toEqual(["r_3"]);
  });

  test("replaces a listed review with the one the event carries", () => {
    const next = applyNotice(listed, notice("viewed", review("r_1", "pending", "renamed")));
    expect(next.map((r) => r.title)).toEqual(["r_3", "renamed"]);
  });

  test("adds a created review only once", () => {
    const next = applyNotice(listed, notice("created", review("r_3", "pending", "again")));
    expect(next.map((r) => [r.id, r.title])).toEqual([
      ["r_3", "again"],
      ["r_1", "r_1"],
    ]);
  });

  test("keeps the same list when nothing on it changes", () => {
    // an event about no review, one about a review not on the list, and an
    // ending of a review that was never listed
    expect(applyNotice(listed, notice("settings_changed", null))).toBe(listed);
    expect(applyNotice(listed, notice("viewed", review("r_9")))).toBe(listed);
    expect(applyNotice(listed, notice("decided", review("r_9", "decided")))).toBe(listed);
    expect(applyNotice(listed, notice("created", null, "r_9"))).toBe(listed);
  });
});
