// The review's bar: the breadcrumb, which the top bar shows, and the
// buttons that act on the review.

import { Ban, Expand, ExternalLink, Maximize2, MessagesSquare, PanelRight, Trash2, X } from "lucide-react";
import { Link } from "react-router";
import { api } from "../../api/client";
import type { Review } from "../../api/types";
import { CopyButton } from "../../components/CopyButton";
import { StarButton } from "../../components/StarButton";
import { sessionOf, sessionPath } from "../../components/Thread";
import { Tooltip } from "../../components/Tooltip";
import { MOD, SHIFT } from "../../lib/keys";
import type { FromSession } from "../../lib/sessions";

/** The link to where the review came from, in a breadcrumb or the panel's head. */
export function OriginLink({ url, originRef }: { url: string; originRef: string | null }) {
  return (
    <Tooltip label={`Open ${originRef ?? "the origin"} in the browser`}>
      <a href={url} target="_blank" rel="noreferrer" className="bar-button crumb-link" aria-label="Open the origin">
        <ExternalLink size={13} />
      </a>
    </Tooltip>
  );
}

/** Where the review was opened from, its title, and its origin's link. */
export function ReviewCrumb({
  title,
  originRef,
  originUrl,
  back,
  backState,
  fromSession,
  sessionsShown,
}: {
  title: string;
  originRef: string | null;
  originUrl: string | null;
  back: { to: string; label: string };
  backState: unknown;
  fromSession: FromSession["session"] | null;
  /** whether the Sessions page is on */
  sessionsShown: boolean;
}) {
  return (
    // it stretches across the bar, so its own empty space moves the window
    <span className="crumb" data-tauri-drag-region>
      {fromSession ? (
        <>
          {sessionsShown ? (
            <>
              <Link to="/sessions" className="crumb-root">
                Sessions
              </Link>
              <span className="crumb-sep">›</span>
            </>
          ) : null}
          <Link
            to={sessionPath(fromSession)}
            className="crumb-root crumb-session truncate"
            title={fromSession.title ?? fromSession.agent}
          >
            {fromSession.title ?? fromSession.agent}
          </Link>
        </>
      ) : (
        <Link to={back.to} state={backState} className="crumb-root">
          {back.label}
        </Link>
      )}
      <span className="crumb-sep">›</span>
      <span className="crumb-title truncate copyable" title={title} data-testid="review-title">
        {title}
      </span>
      {originUrl ? <OriginLink url={originUrl} originRef={originRef} /> : null}
    </span>
  );
}

/** The buttons that act on the review. */
export function ReviewActions({
  review,
  hasView,
  inPanel,
  showInPanel,
  sessionLink,
  onDiscard,
  onDelete,
  onMaximize,
  onOpenPage,
  onShowInPanel,
  onClose,
}: {
  review: Review;
  /** whether a plugin's view shows it, which can be maximized */
  hasView: boolean;
  inPanel: boolean;
  /** whether it can return to the inbox's side panel */
  showInPanel: boolean;
  /** whether to link to its session's thread */
  sessionLink: boolean;
  onDiscard: () => void;
  onDelete: () => void;
  onMaximize: () => void;
  onOpenPage: () => void;
  onShowInPanel: () => void;
  onClose?: () => void;
}) {
  const question = review.kind === "question";
  const deleteLabel = question ? "Delete these questions" : "Delete this review";
  return (
    <>
      {review.status === "pending" ? (
        <Tooltip label="Discard: the agent is told to stop" keys={[MOD, "D"]} side="bottom">
          <button type="button" className="bar-button" onClick={onDiscard} aria-label="Discard this review">
            <Ban size={15} />
          </button>
        </Tooltip>
      ) : null}
      <CopyButton
        key={review.id}
        text={() => api.reviewMarkdown(review.id)}
        label="Copy decision as markdown"
        ariaLabel="Copy the decision as markdown"
        className="bar-button copy-button"
        size={15}
      />
      {question ? null : <StarButton review={review} />}
      <Tooltip label={deleteLabel} side="bottom">
        <button type="button" className="bar-button" onClick={onDelete} aria-label={deleteLabel}>
          <Trash2 size={15} />
        </button>
      </Tooltip>
      {sessionLink ? <SessionLink review={review} /> : null}
      {hasView ? (
        <Tooltip label="Maximize the view" keys={[MOD, SHIFT, "M"]} side="bottom">
          <button type="button" className="bar-button" onClick={onMaximize} aria-label="Maximize the view">
            <Maximize2 size={15} />
          </button>
        </Tooltip>
      ) : null}
      {inPanel ? (
        <Tooltip label="Open as a page" side="bottom">
          <button type="button" className="bar-button" onClick={onOpenPage} aria-label="Open as a page">
            <Expand size={15} />
          </button>
        </Tooltip>
      ) : showInPanel ? (
        <Tooltip label="Show in the inbox's panel" side="bottom">
          <button type="button" className="bar-button" onClick={onShowInPanel} aria-label="Show in the inbox's panel">
            <PanelRight size={15} />
          </button>
        </Tooltip>
      ) : null}
      {inPanel && onClose ? (
        <Tooltip label="Close" keys={["Esc"]} side="bottom">
          <button type="button" className="bar-button" onClick={onClose} aria-label="Close the panel">
            <X size={15} />
          </button>
        </Tooltip>
      ) : null}
    </>
  );
}

/** A link to the thread of the session the review was sent from, if the
 *  agent said which. */
function SessionLink({ review }: { review: Review }) {
  const session = sessionOf(review);
  if (!session) return null;
  return (
    <Tooltip label="Open the session's thread" side="bottom">
      <Link to={sessionPath(session)} className="bar-button" aria-label="Open the session's thread">
        <MessagesSquare size={15} />
      </Link>
    </Tooltip>
  );
}
