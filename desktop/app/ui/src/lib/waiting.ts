// Which waiting reviews the inbox's filters keep: the search, the project or
// repository, the plugin and the star. The Pending and New rounds tabs, and
// their counts, all go by it.

import type { Review } from "../api/types";
import { reviewLabel } from "../components/ReviewIcon";
import { inScope, scopeOf } from "./url";

/** Whether a review holds the searched text in what its row shows. */
export function matches(review: Review, q: string) {
  if (!q) return true;
  const text = [
    review.title,
    reviewLabel(review),
    review.requested_by,
    review.origin.repo,
    review.origin.workflow,
    review.origin.ref,
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
  return text.includes(q.toLowerCase());
}

/** The filters in the address, with the search box's text, as one test. */
export function waitingFilter(params: URLSearchParams, q: string) {
  const scope = scopeOf(params);
  const plugin = params.get("plugin") ?? "";
  const starred = params.get("starred") === "1";
  return (r: Review) =>
    matches(r, q) &&
    inScope(r.origin.repo, r.project, scope) &&
    (!plugin || r.plugin === plugin) &&
    (!starred || !!r.starred);
}

/** Whether any of those filters is set. */
export const filtering = (params: URLSearchParams, q: string) =>
  !!(q || scopeOf(params) || params.get("plugin") || params.get("starred") === "1");
