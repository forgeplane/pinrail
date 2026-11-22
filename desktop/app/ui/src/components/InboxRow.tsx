// One row of the inbox: a review or a question that waits, with its icon,
// what happened, its items, files, counts, plugin and age, and its star and
// discard buttons.
// Memoized, with handlers that take the review, so that moving the pointer
// down the list draws again only the rows whose highlight changed.

import { Ban, MessageCircleQuestion } from "lucide-react";
import { memo, type MouseEvent } from "react";
import { Link, type To } from "react-router";
import type { Review } from "../api/types";
import { age } from "../lib/format";
import { MOD } from "../lib/keys";
import { FilesCount, SummaryCounts } from "./Badges";
import { ItemChips } from "./ItemChip";
import { ReviewIcon, reviewLabel } from "./ReviewIcon";
import { StarButton } from "./StarButton";
import { BADGE, SubjectIcon } from "./SubjectIcon";
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

/** What happened, as the line under the title says it: who asks, sent a
 *  new round or requested the review. */
function whatHappened(review: Review): string {
  const by = review.requested_by;
  if (review.kind === "question") return by ? `${by} asks you` : "A question for you";
  if (review.revises) return by ? `${by} sent a new round` : "A new round";
  return by ? `${by} requested a review` : "A review for you";
}

export const InboxRow = memo(function InboxRow({ review, here, focused, open, showRepo, hrefFor, actions }: Props) {
  return (
    <Link
      to={hrefFor(review.id)}
      onClick={(event) => actions.open(event, review.id)}
      className={`review-row has-icon ${review.kind === "question" ? "is-question" : ""} ${focused ? "is-focused" : ""} ${open ? "is-open" : ""}`}
      aria-current={open ? "true" : undefined}
      onMouseEnter={() => actions.hover(here)}
      data-testid="review-row"
      data-row={here}
    >
      <span className="review-row-marker" aria-label="Pending" />
      {/* questions an agent asked are marked apart from the reviews it sent */}
      {review.kind === "question" ? (
        <SubjectIcon badge={BADGE.asks}>
          <MessageCircleQuestion size={16} aria-hidden="true" />
        </SubjectIcon>
      ) : (
        <SubjectIcon badge={review.revises ? BADGE.round : null}>
          <ReviewIcon review={review} size={16} />
        </SubjectIcon>
      )}
      <span className="review-row-main">
        <span className="review-row-title truncate">
          {review.title}
          {review.origin.ref ? <span className="review-row-ref">{review.origin.ref}</span> : null}
        </span>
        <span className="review-row-meta">
          <span className="update-row-reason">{whatHappened(review)}</span>
          <ItemChips items={review.items ?? []} />
          {review.origin.workflow ? (
            <>
              <span>·</span>
              <span>{review.origin.workflow}</span>
            </>
          ) : null}
          <FilesCount total={review.attachments_total} />
          {showRepo ? (
            <>
              <span>·</span>
              <span className="review-row-project" data-testid="review-row-project">
                {review.origin.repo ?? "No repository"}
              </span>
            </>
          ) : null}
        </span>
      </span>
      <span className="review-row-summary">
        <SummaryCounts summary={review.summary} />
      </span>
      {review.kind === "question" ? (
        <span className="review-row-plugin">
          <MessageCircleQuestion size={13} aria-hidden="true" />
          question
        </span>
      ) : (
        <span className="review-row-plugin" title={reviewLabel(review)}>
          <ReviewIcon review={review} size={13} />
          {reviewLabel(review)}
        </span>
      )}
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
