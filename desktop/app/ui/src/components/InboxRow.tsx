// One row of the inbox: a review or a question that waits, with its agent,
// items, files, plugin, counts and age, and its star and discard buttons.
// Memoized, with handlers that take the review, so that moving the pointer
// down the list draws again only the rows whose highlight changed.

import { Ban, MessageCircleQuestion } from "lucide-react";
import { memo, type MouseEvent } from "react";
import { Link, type To } from "react-router";
import type { Review } from "../api/types";
import { age } from "../lib/format";
import { MOD } from "../lib/keys";
import { AgentIcon } from "./AgentIcon";
import { FilesCount, SummaryCounts } from "./Badges";
import { ItemChips } from "./ItemChip";
import { ReviewIcon, reviewLabel } from "./ReviewIcon";
import { StarButton } from "./StarButton";
import { Tooltip } from "./Tooltip";

/** What a row does, the same functions from one render to the next. */
export type InboxRowActions = {
  hover: (here: number) => void;
  open: (event: MouseEvent, id: string) => void;
  discard: (review: Review) => void;
};

type Props = {
  review: Review;
  /** its place in the order J and K walk */
  here: number;
  focused: boolean;
  open: boolean;
  /** whether the row names its repository */
  showRepo: boolean;
  hrefFor: (id: string) => To;
  actions: InboxRowActions;
};

export const InboxRow = memo(function InboxRow({ review, here, focused, open, showRepo, hrefFor, actions }: Props) {
  return (
    <Link
      to={hrefFor(review.id)}
      onClick={(event) => actions.open(event, review.id)}
      className={`review-row ${review.kind === "question" ? "is-question" : ""} ${focused ? "is-focused" : ""} ${open ? "is-open" : ""}`}
      aria-current={open ? "true" : undefined}
      onMouseEnter={() => actions.hover(here)}
      data-testid="review-row"
      data-row={here}
    >
      {/* questions an agent asked are marked apart from the reviews it sent */}
      {review.kind === "question" ? (
        <MessageCircleQuestion className="review-row-asks" size={15} aria-label="Asks you" />
      ) : (
        <span className="review-row-marker" aria-label="Pending" />
      )}
      <span className="review-row-main">
        <span className="review-row-title truncate">
          {review.title}
          {review.origin.ref ? <span className="review-row-ref">{review.origin.ref}</span> : null}
        </span>
        <span className="review-row-meta">
          {showRepo ? (
            <span className="review-row-project" data-testid="review-row-project">
              {review.origin.repo ?? "No repository"}
            </span>
          ) : null}
          {showRepo && (review.requested_by || review.origin.workflow) ? <span>·</span> : null}
          {review.requested_by ? (
            <span className="requester">
              <AgentIcon requestedBy={review.requested_by} />
              {review.requested_by}
            </span>
          ) : null}
          <ItemChips items={review.items ?? []} />
          {review.requested_by && review.origin.workflow ? <span>·</span> : null}
          {review.origin.workflow ? <span>{review.origin.workflow}</span> : null}
          {review.revises ? <span>· New round</span> : null}
          <FilesCount total={review.attachments_total} />
        </span>
      </span>
      {review.kind === "question" ? (
        <span className="review-row-plugin">
          <span className="asks-chip pill">asks you</span>
        </span>
      ) : (
        <span className="review-row-plugin" title={reviewLabel(review)}>
          <ReviewIcon review={review} size={13} />
          {reviewLabel(review)}
        </span>
      )}
      <span className="review-row-summary">
        <SummaryCounts summary={review.summary} />
      </span>
      <span className="review-row-end">
        <time className="review-row-time" dateTime={review.created_at}>
          {age(review.created_at)}
        </time>
        {review.kind === "question" ? null : <StarButton review={review} inRow ofReview />}
        <Tooltip label="Discard: the agent is told to stop" keys={[MOD, "D"]} side="top">
          <button
            type="button"
            className="bar-button"
            aria-label={`Discard ${review.title}`}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              actions.discard(review);
            }}
          >
            <Ban size={14} />
          </button>
        </Tooltip>
      </span>
    </Link>
  );
});
