// One of the person's notifications in the inbox, other than a review that
// waits, which keeps its own row: a work item, a session, or a review that
// ended. A row is bold with a dot while it is unread; its buttons mark it
// read or unread and, when it does not wait, done.

import { Check, ListChecks, Mail, MailOpen, MessageCircleQuestion, MessagesSquare, Undo2 } from "lucide-react";
import { memo, type MouseEvent, type ReactNode } from "react";
import { Link, type To } from "react-router";
import type { Notification } from "../api/types";
import { age, stamp } from "../lib/format";
import { sessionLabel } from "../lib/sessions";
import { lineOf } from "../state/inboxUpdates";
import { AgentIcon } from "./AgentIcon";
import { itemPath } from "./ItemChip";
import { ItemStatusIcon } from "./ItemStatusIcon";
import { ReviewIcon, reviewLabel } from "./ReviewIcon";
import { SubjectIcon, badgeOf } from "./SubjectIcon";
import { sessionPath } from "./Thread";
import { Tooltip } from "./Tooltip";

/** What a notification's row does, the same functions from one render to
 *  the next. */
export type NotificationActions = {
  hover: (here: number) => void;
  /** a click on the row's link: opens the subject, or with the side panel
   *  takes the click to open it there */
  open: (event: MouseEvent, n: Notification) => void;
  setRead: (n: Notification, read: boolean) => void;
  /** marks it done, or not done when it is */
  setDone: (n: Notification) => void;
};

/** What the inbox's `open` names for a notification's subject in the side
 *  panel: a review's id, `item:KEY` or `session:AGENT/ID`. */
export function panelToken(n: Notification): string {
  switch (n.subject.kind) {
    case "item":
      return `item:${n.subject.item.key}`;
    case "session":
      return `session:${n.subject.session.agent}/${n.subject.session.id}`;
    case "review":
      return n.subject.review.id;
  }
}

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

/** The subject's title, a work item's status, its key or reference, its
 *  icon and its kind. */
function subjectOf(n: Notification): {
  title: string;
  status?: ReactNode;
  tag?: string;
  mark: ReactNode;
  kind: ReactNode;
} {
  switch (n.subject.kind) {
    case "item":
      return {
        title: n.subject.item.title ?? "A work item",
        status: n.subject.item.status ? (
          <span className="review-row-status">
            <ItemStatusIcon status={n.subject.item.status} size={12} />
          </span>
        ) : undefined,
        tag: n.subject.item.key,
        mark: <ListChecks size={16} aria-hidden="true" />,
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
        mark: <AgentIcon requestedBy={session.agent} size={16} />,
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
        mark:
          review.kind === "question" ? (
            <MessageCircleQuestion size={16} aria-hidden="true" />
          ) : (
            <ReviewIcon review={review} size={16} />
          ),
        kind: review.plugin !== undefined ? reviewLabel(review) : "review",
      };
    }
  }
}

export const NotificationRow = memo(function NotificationRow({
  notification: n,
  here,
  focused,
  href,
  actions,
}: {
  notification: Notification;
  /** its place in the order J and K walk */
  here: number;
  focused: boolean;
  href: To;
  actions: NotificationActions;
}) {
  const subject = subjectOf(n);
  const line = lineOf(n);
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
      onClick={(event) => actions.open(event, n)}
      className={`review-row update-row ${n.read ? "is-read" : "is-unread"} ${n.waiting ? "is-waiting" : ""} ${focused ? "is-focused" : ""}`}
      onMouseEnter={() => actions.hover(here)}
      data-testid="update-row"
      data-row={here}
    >
      {n.read ? <span /> : <span className="review-row-marker" aria-label="Unread" />}
      <SubjectIcon badge={badgeOf(n)}>{subject.mark}</SubjectIcon>
      <span className="review-row-main">
        <span className="review-row-title truncate">
          {subject.title}
          {subject.status}
          {subject.tag ? <span className="review-row-ref">{subject.tag}</span> : null}
        </span>
        <span className="review-row-meta">
          <span className="update-row-reason">{line.text}</span>
          {line.detail ? (
            <>
              <span>·</span>
              <span className="review-row-project">{line.detail}</span>
            </>
          ) : null}
          {line.more ? (
            <>
              <span>·</span>
              <span>and {line.more === 1 ? "1 more change" : `${line.more} more changes`}</span>
            </>
          ) : null}
          {line.under ? (
            <>
              <span>·</span>
              <span>Under {line.under}</span>
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
        {n.waiting
          ? null
          : n.done
            ? button("Mark as not done", ["E"], <Undo2 size={14} />, () => actions.setDone(n))
            : button("Mark as done", ["E"], <Check size={14} />, () => actions.setDone(n))}
      </span>
    </Link>
  );
});
