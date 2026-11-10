// Deleting a work item, with its sub-items at any depth. Its reviews stay,
// with no item. Esc leaves.

import { useRef, useState } from "react";
import { api } from "../api/client";
import type { WorkItem } from "../api/types";
import { Dialog } from "./Dialog";

export function DeleteItemDialog({
  item,
  onClose,
  onDone,
}: {
  item: WorkItem;
  onClose: () => void;
  onDone: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const subs = item.sub_items.total;

  const confirm = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await api.deleteItem(item.key);
      onDone();
    } catch (e) {
      setError(e instanceof Error ? e.message : "The item was not deleted.");
      setBusy(false);
    }
  };

  return (
    <Dialog labelledBy="delete-item-title" className="discard-dialog" initialFocus={cancel} onClose={onClose}>
      <div className="dialog-head">
        <h2 id="delete-item-title">Delete {item.key}?</h2>
      </div>
      <p className="dim discard-what truncate" title={item.title}>
        {item.title}
      </p>
      <p className="dim">
        The item is deleted from Pinrail, with its comments and files. Its reviews and questions stay, belonging to no
        item. This cannot be undone.
      </p>
      {subs ? (
        <p className="dim" data-testid="delete-item-subs">
          Its {subs === 1 ? "sub-item goes" : `${subs} sub-items go`} with it, and theirs too.
        </p>
      ) : null}
      {error ? <p className="notice notice-danger">{error}</p> : null}
      <div className="dialog-actions">
        <button ref={cancel} type="button" className="chrome-button" onClick={onClose}>
          Cancel
        </button>
        <button type="button" className="chrome-button button-danger" onClick={confirm} disabled={busy}>
          Delete item
        </button>
      </div>
    </Dialog>
  );
}
