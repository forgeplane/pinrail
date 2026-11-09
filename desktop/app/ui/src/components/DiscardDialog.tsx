// Discarding a review: the person's "no, and stop". Nothing is decided and
// nothing is posted; the agent waiting on the review is told, with the
// reason when one is given. Esc leaves.

import { useRef, useState } from "react";
import { api } from "../api/client";
import type { Review } from "../api/types";
import { composing } from "../lib/keys";
import { Dialog } from "./Dialog";

export function DiscardDialog({
  review,
  onClose,
  onDone,
}: {
  review: Review;
  onClose: () => void;
  onDone: (review: Review) => void;
}) {
  // questions are declined, in the words of a person asked something
  const question = review.kind === "question";
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const field = useRef<HTMLTextAreaElement>(null);

  const confirm = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      onDone(await api.discard(review.id, reason.trim() || undefined));
    } catch (e) {
      setError(
        e instanceof Error
          ? e.message
          : question
            ? "The questions were not declined."
            : "The review was not discarded.",
      );
      setBusy(false);
    }
  };

  return (
    <Dialog labelledBy="discard-title" className="discard-dialog" initialFocus={field} onClose={onClose}>
      <div className="dialog-head">
        <h2 id="discard-title">{question ? "Decline to answer?" : "Discard this review?"}</h2>
      </div>
      <p className="dim discard-what" title={review.title}>
        {review.title}
      </p>
      <p className="dim">
        {question
          ? "The agent is told to stop the work it was asking about. None of the questions are answered."
          : "The agent is told to stop the work it was asking about. Nothing is decided and nothing is posted."}
      </p>
      <textarea
        ref={field}
        className="discard-reason"
        rows={2}
        placeholder="Why, for the agent (optional)"
        aria-label="Reason"
        value={reason}
        onChange={(e) => setReason(e.target.value)}
        onKeyDown={(e) => {
          if (composing(e)) return;
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
        <button type="button" className="chrome-button button-danger" onClick={confirm} disabled={busy}>
          {busy ? (question ? "Declining…" : "Discarding…") : question ? "Decline" : "Discard"}
        </button>
      </div>
    </Dialog>
  );
}
