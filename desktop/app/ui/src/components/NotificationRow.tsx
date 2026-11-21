// One of the person's notifications in the inbox, other than a review that
// waits, which keeps its own row: a work item, a session, or a review that
// ended. A row is bold with a dot while it is unread; its buttons mark it
// read or unread and, when it does not wait, done.

import { Check, ChevronDown, ChevronRight, ListChecks, Mail, MailOpen, MessagesSquare } from "lucide-react";
import { memo, type ReactNode } from "react";
import { Link, type To } from "react-router";
import type { Notification } from "../api/types";
import { age, stamp } from "../lib/format";
import { sessionLabel } from "../lib/sessions";
import { reasonOf } from "../state/inboxUpdates";
import { AgentIcon } from "./AgentIcon";
import { itemPath } from "./ItemChip";
import { ItemStatusIcon } from "./ItemStatusIcon";
import { ReviewIcon, reviewLabel } from "./ReviewIcon";
import { sessionPath } from "./Thread";
import { Tooltip } from "./Tooltip";

/** What a notification's row does, the same functions from one render to
 *  the next. */
export type NotificationActions = {
  hover: (here: number) => void;
  open: (n: Notification) => void;
  setRead: (n: Notification, read: boolean) => void;
  setDone: (n: Notification) => void;
  /** unfolds the row to list its new events, or folds it */
  toggle: (n: Notification) => void;
};

/** Whether a row unfolds: a work item's or a session's, with more than
 *  one new event. */
export const unfolds = (n: Notification) => n.unread > 1 && n.subject.kind !== "review";

/** Where a notification leads: its work item, its session, or its review
 *  as `reviewHref` opens it. */
export function notificationPath(n: Notification, reviewHref: (id: string) => To): To {
  switch (n.subject.kind) {
    case "item":
      return itemPath(n.subject.item.key);
    case "session":
      return sessionPath(n.subject.session);
    case "review":
      return reviewHref(n.subject.review.id);
  }
}

/** The subject's title, its key or reference, its mark and its kind. */
function subjectOf(n: Notification): { title: string; tag?: string; mark: ReactNode; kind: ReactNode } {
  switch (n.subject.kind) {
    case "item":
      return {
        title: n.subject.item.title ?? "A work item",
        tag: n.subject.item.key,
        mark: n.subject.item.status ? <ItemStatusIcon status={n.subject.item.status} /> : <span />,
        kind: (
          <>
            <ListChecks size={13} aria-hidden="true" />
            work item
          </>
        ),
      };
    case "session": {
      const session = n.subject.session;
      return {
        title: session.agent ? sessionLabel(session) : "A session",
        mark: <AgentIcon requestedBy={session.agent} size={15} />,
        kind: (
          <>
            <MessagesSquare size={13} aria-hidden="true" />
            session
          </>
        ),
      };
    }
    case "review": {
      const review = n.subject.review;
      return {
        title: review.title ?? "A review",
        tag: review.origin?.ref ?? undefined,
        mark: review.plugin !== undefined ? <ReviewIcon review={review} size={14} /> : <span />,
        kind: review.plugin !== undefined ? reviewLabel(review) : "review",
      };
    }
  }
}

export const NotificationRow = memo(function NotificationRow({
  notification: n,
  here,
  focused,
  unfolded,
  href,
  actions,
}: {
  notification: Notification;
  /** its place in the order J and K walk */
  here: number;
  focused: boolean;
  /** whether it lists its new events below */
  unfolded: boolean;
  href: To;
  actions: NotificationActions;
}) {
  const subject = subjectOf(n);
  const button = (label: string, keys: string[], icon: ReactNode, act: () => void) => (
    <Tooltip label={label} keys={keys} side="top">
      <button
        type="button"
        className="bar-button"
        aria-label={`${label}: ${subject.title}`}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          act();
        }}
      >
        {icon}
      </button>
    </Tooltip>
  );
  return (
    <Link
      to={href}
      onClick={() => actions.open(n)}
      className={`review-row update-row ${n.read ? "is-read" : "is-unread"} ${n.waiting ? "is-waiting" : ""} ${focused ? "is-focused" : ""}`}
      onMouseEnter={() => actions.hover(here)}
      data-testid="update-row"
      data-row={here}
    >
      {n.read ? <span /> : <span className="review-row-marker" aria-label="Unread" />}
      {subject.mark}
      <span className="review-row-main">
        <span className="review-row-title truncate">
          {unfolds(n) ? (
            <button
              type="button"
              className="update-row-fold"
              aria-expanded={unfolded}
              aria-label={unfolded ? `Fold ${subject.title}` : `Show the new events of ${subject.title}`}
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                actions.toggle(n);
              }}
            >
              {unfolded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
            </button>
          ) : null}
          {subject.title}
          {subject.tag ? <span className="review-row-ref">{subject.tag}</span> : null}
          {n.unread > 1 ? <span className="update-row-new pill">{n.unread} new</span> : null}
        </span>
        <span className="review-row-meta">
          <span className="update-row-reason">{reasonOf(n)}</span>
          {n.summary ? (
            <>
              <span>·</span>
              <span className="review-row-project">{n.summary}</span>
            </>
          ) : null}
          {n.repo ? (
            <>
              <span>·</span>
              <span>{n.repo}</span>
            </>
          ) : null}
        </span>
      </span>
      <span className="review-row-plugin">{subject.kind}</span>
      <span className="review-row-end">
        <time className="review-row-time" dateTime={n.updated_at} title={stamp(n.updated_at)}>
          {age(n.updated_at)}
        </time>
        {n.read
          ? button("Mark as unread", ["U"], <Mail size={14} />, () => actions.setRead(n, false))
          : button("Mark as read", ["U"], <MailOpen size={14} />, () => actions.setRead(n, true))}
        {/* what waits ends when the person acts on it, not when they file it */}
        {n.waiting ? null : button("Mark as done", ["E"], <Check size={14} />, () => actions.setDone(n))}
      </span>
    </Link>
  );
});
