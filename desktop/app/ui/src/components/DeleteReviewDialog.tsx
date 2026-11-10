// Deleting a review, with every round of it, from the history or from its
// screen. A round still waiting is discarded first, so the agent is told to
// stop. Esc leaves.

import { useRef, useState } from "react";
import { api } from "../api/client";
import type { Review } from "../api/types";
import { Dialog } from "./Dialog";

export function DeleteReviewDialog({
  review,
  onClose,
  onDone,
}: {
  review: Review;
  onClose: () => void;
  onDone: () => void;
}) {
  const question = review.kind === "question";
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const cancel = useRef<HTMLButtonElement>(null);

  const confirm = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await api.deleteReview(review.id);
      onDone();
    } catch (e) {
      setError(e instanceof Error ? e.message : "The review was not deleted.");
      setBusy(false);
    }
  };

  return (
    <Dialog labelledBy="delete-review-title" className="discard-dialog" initialFocus={cancel} onClose={onClose}>
      <div className="dialog-head">
        <h2 id="delete-review-title">{question ? "Delete these questions?" : "Delete this review?"}</h2>
      </div>
      <p className="dim discard-what truncate" title={review.title}>
        {review.title}
      </p>
      <p className="dim">
        {question
          ? "The questions and their answers are deleted from Pinrail. This cannot be undone."
          : "Every round of the review is deleted from Pinrail, with its decisions and comments. This cannot be undone."}
      </p>
      {review.status === "pending" ? (
        <p className="dim" data-testid="delete-review-waiting">
          {question ? "They still wait" : "It still waits"}: the agent is told to stop first.
        </p>
      ) : null}
      {review.starred ? <p className="dim">The review is starred.</p> : null}
      {error ? <p className="notice notice-danger">{error}</p> : null}
      <div className="dialog-actions">
        <button ref={cancel} type="button" className="chrome-button" onClick={onClose}>
          Cancel
        </button>
        <button type="button" className="chrome-button button-danger" onClick={confirm} disabled={busy}>
          {question ? "Delete questions" : "Delete review"}
        </button>
      </div>
    </Dialog>
  );
}
