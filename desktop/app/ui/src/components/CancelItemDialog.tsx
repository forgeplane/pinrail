// Cancelling a work item that has open sub-items: they are cancelled with
// it, at any depth, so the person is asked first. Esc leaves.

import { useEffect, useRef } from "react";
import type { WorkItem } from "../api/types";

/** How many of an item's own sub-items are still open or in progress. */
export const openSubItems = (item: WorkItem) => item.sub_items.total - item.sub_items.closed;

export function CancelItemDialog({
  item,
  onClose,
  onConfirm,
}: {
  item: WorkItem;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const keep = useRef<HTMLButtonElement>(null);
  const open = openSubItems(item);

  useEffect(() => {
    keep.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  return (
    <div className="app-dialog-backdrop" onMouseDown={onClose}>
      <div
        className="app-dialog discard-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="cancel-item-title"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="dialog-head">
          <h2 id="cancel-item-title">Cancel {item.key}?</h2>
        </div>
        <p className="dim discard-what" title={item.title}>
          {item.title}
        </p>
        <p className="dim" data-testid="cancel-item-subs">
          {open === 1 ? "Its open sub-item is" : `Its ${open} open sub-items are`} cancelled with it, and any of theirs
          that are still open. Sub-items that are done stay done.
        </p>
        <div className="dialog-actions">
          <button ref={keep} type="button" className="chrome-button" onClick={onClose}>
            Keep it
          </button>
          <button type="button" className="chrome-button button-danger" onClick={onConfirm}>
            Cancel {open === 1 ? "both" : "them all"}
          </button>
        </div>
      </div>
    </div>
  );
}
