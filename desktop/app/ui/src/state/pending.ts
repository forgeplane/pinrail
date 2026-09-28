// How an event from the server changes the list of pending reviews, kept
// apart from the live state so it can be tested on its own.

import type { Notice, Review } from "../api/types";

/** Events after which a review is no longer pending. */
export const ENDINGS = new Set(["decided", "withdrawn", "discarded", "expired"]);

/**
 * The pending list after an event about one review. The event carries the
 * review as it now stands, so the list changes without asking the server:
 * a review that ends leaves it, a new one joins it in its place (newest
 * first, as the server lists them), and any other change replaces it.
 */
export function applyNotice(pending: Review[], notice: Notice): Review[] {
  const id = notice.review_id;
  if (!id) return pending;
  const listed = pending.some((r) => r.id === id);
  const review = notice.review;
  if (ENDINGS.has(notice.kind) || (review && review.status !== "pending")) {
    return listed ? pending.filter((r) => r.id !== id) : pending;
  }
  if (!review) return pending;
  if (listed) return pending.map((r) => (r.id === id ? review : r));
  if (notice.kind !== "created") return pending;
  return [review, ...pending].sort((a, b) => (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
}
