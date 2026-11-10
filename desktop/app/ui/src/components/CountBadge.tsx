// A count in a small pill, such as the inbox's number of waiting reviews.
// The digits sit in a box trimmed to their own height, from the top of a
// capital to the baseline, so they are centred to the pixel beside an icon
// and in the pill, whatever line metrics the engine takes from the font.

import type { HTMLAttributes, ReactNode } from "react";

/** How the count stands out: on the accent colour, or as plain text. */
type Tone = "accent" | "quiet" | "plain";

export function CountBadge({
  count,
  tone = "accent",
  icon,
  className = "",
  ...rest
}: Omit<HTMLAttributes<HTMLSpanElement>, "children"> & {
  count: number;
  tone?: Tone;
  /** a small picture before the number, such as a status */
  icon?: ReactNode;
}) {
  return (
    <span {...rest} className={`count-badge is-${tone} ${className}`.trim()}>
      {icon}
      <span className="count-badge-number">{count}</span>
    </span>
  );
}
