// A review's icon with its status as a small badge on the top right corner:
// an amber dot while it waits, then a check, a cross, a dash or a clock for
// decided, discarded, withdrawn and expired. The status is the badge's
// accessible name and the icon's tooltip.

import { Check, Clock3, Minus, X, type LucideIcon } from "lucide-react";
import type { Review, Status } from "../api/types";
import { ReviewIcon } from "./ReviewIcon";

const STATUS: Record<Status, { label: string; Glyph: LucideIcon | null }> = {
  pending: { label: "Waiting", Glyph: null },
  decided: { label: "Decided", Glyph: Check },
  discarded: { label: "Discarded", Glyph: X },
  withdrawn: { label: "Withdrawn", Glyph: Minus },
  expired: { label: "Expired", Glyph: Clock3 },
};

export function ReviewStatusIcon({
  review,
  size = 15,
}: {
  review: Pick<Review, "kind" | "plugin" | "status">;
  size?: number;
}) {
  const { label, Glyph } = STATUS[review.status];
  return (
    <span className={`review-status-icon is-${review.status}`} title={label}>
      <ReviewIcon review={review} size={size} />
      <span className="review-status-badge" role="img" aria-label={label} data-status={review.status}>
        {Glyph ? <Glyph size={7} strokeWidth={4} aria-hidden /> : null}
      </span>
    </span>
  );
}
