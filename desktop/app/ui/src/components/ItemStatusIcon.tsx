// A work item's status as a small icon in its colour: a ring when open, a
// half-filled ring in progress, a ticked disc when done, a crossed ring
// when cancelled.

import type { ItemStatus } from "../api/types";

export const STATUS_LABEL: Record<ItemStatus, string> = {
  open: "Open",
  in_progress: "In progress",
  done: "Done",
  cancelled: "Cancelled",
};

export function ItemStatusIcon({ status, size = 14 }: { status: ItemStatus; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      className={`item-status is-${status}`}
      role="img"
      aria-label={STATUS_LABEL[status]}
    >
      {status === "done" ? (
        <>
          <circle cx="8" cy="8" r="7" fill="currentColor" />
          <path
            d="m5 8.2 2 2 4-4.2"
            fill="none"
            stroke="var(--bg)"
            strokeWidth="1.7"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </>
      ) : (
        <>
          <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="1.6" />
          {status === "in_progress" ? <path d="M8 4a4 4 0 0 1 0 8z" fill="currentColor" /> : null}
          {status === "cancelled" ? (
            <path
              d="m5.8 5.8 4.4 4.4M10.2 5.8l-4.4 4.4"
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="round"
            />
          ) : null}
        </>
      )}
    </svg>
  );
}
