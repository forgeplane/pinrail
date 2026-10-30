// The star of a review: one button that shows whether the review is starred
// and toggles the star. A star belongs to one round, and a review is starred
// when any of its rounds is, so a round can show a star another round holds.

import { Star } from "lucide-react";
import { useCallback } from "react";
import { api } from "../api/client";
import type { Review } from "../api/types";
import { MOD, SHIFT } from "../lib/keys";
import { useToast } from "../state/toasts";
import { Tooltip } from "./Tooltip";

/** Stars the round, or removes its star; the event stream updates the views. */
export function useToggleStar(): (review: Review) => void {
  const toast = useToast();
  return useCallback(
    (review: Review) => {
      api.star(review.id, !review.starred_at).catch(() => toast("Could not change the star", "danger"));
    },
    [toast],
  );
}

export function StarButton({
  review,
  inRow = false,
  ofReview = false,
}: {
  review: Review;
  /** on a list's row rather than in a toolbar */
  inRow?: boolean;
  /** for a list of each review's newest round, as the inbox: filled when
   *  any round of the review is starred */
  ofReview?: boolean;
}) {
  const toggle = useToggleStar();
  const own = !!review.starred_at;
  // elsewhere, the star another round holds, which only that round removes
  const elsewhere = !own && !!review.starred;
  const filled = own || (ofReview && elsewhere);
  const disabled = ofReview && elsewhere;
  const label = own
    ? "Remove the star"
    : disabled
      ? "An earlier round is starred"
      : elsewhere
        ? "Star this round; another round is starred"
        : "Star";
  return (
    <Tooltip label={label} keys={inRow ? undefined : [MOD, SHIFT, "S"]} side={inRow ? "top" : "bottom"}>
      <button
        type="button"
        className={`bar-button star-button ${filled ? "is-starred" : ""}`}
        aria-label={label}
        aria-pressed={own}
        aria-disabled={disabled || undefined}
        data-testid="star"
        onClick={(event) => {
          // on a row, the click stays off the row's link
          event.preventDefault();
          event.stopPropagation();
          if (!disabled) toggle(review);
        }}
      >
        <Star size={inRow ? 14 : 15} fill={filled ? "currentColor" : "none"} />
      </button>
    </Tooltip>
  );
}
