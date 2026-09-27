// Discarding a review: the person's "no, and stop". Nothing is decided and
// nothing is posted; the agent waiting on the review is told, with the
// reason when one is given. Enter confirms, Esc leaves.

import { useEffect, useRef, useState } from "react";
import { api } from "../api/client";
import type { Review } from "../api/types";

export function DiscardDialog({ review, onClose, onDone }: { review: Review; onClose: () => void; onDone: (review: Review) => void }) {
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const field = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    field.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  const confirm = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      onDone(await api.discard(review.id, reason.trim() || undefined));
    } catch (e) {
      setError(e instanceof Error ? e.message : "The review was not discarded.");
      setBusy(false);
    }
  };

  return (
    <div className="app-dialog-backdrop" onMouseDown={onClose}>
      <div className="app-dialog discard-dialog" role="dialog" aria-modal="true" aria-labelledby="discard-title" onMouseDown={(e) => e.stopPropagation()}>
        <div className="dialog-head">
          <h2 id="discard-title">Discard this review?</h2>
        </div>
        <p className="dim discard-what" title={review.title}>
          {review.title}
        </p>
        <p className="dim">The agent is told to stop the work it was asking about. Nothing is decided and nothing is posted.</p>
        <textarea
          ref={field}
          className="discard-reason"
          rows={2}
          placeholder="Why, for the agent (optional)"
          aria-label="Reason"
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              confirm();
            }
          }}
        />
        {error ? <p className="notice notice-danger">{error}</p> : null}
        <div className="dialog-actions">
          <button type="button" className="chrome-button" onClick={onClose} disabled={busy}>
            Keep it
          </button>
          <button type="button" className="chrome-button button-danger" onClick={confirm} disabled={busy} data-discard-confirm>
            {busy ? "Discarding…" : "Discard"}
          </button>
        </div>
      </div>
    </div>
  );
}
