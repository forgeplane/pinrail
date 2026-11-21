// The inbox's Updates: the person's notifications that do not wait for
// them, such as an agent's comment on a work item, newest first, under the
// reviews that wait. A row is bold with a dot while it is unread; its
// buttons mark it read or unread, and done.

import { Check, ListChecks, Mail, MailOpen } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useState } from "react";
import { Link } from "react-router";
import { api } from "../api/client";
import type { Notification } from "../api/types";
import { age, stamp } from "../lib/format";
import { useLive, useNotice } from "../state/live";
import { applyNotification, listed, reasonOf } from "../state/inboxUpdates";
import { itemPath } from "./ItemChip";
import { ItemStatusIcon } from "./ItemStatusIcon";
import { Tooltip } from "./Tooltip";

/** How many updates the inbox shows. */
const SHOWN = 50;

export type Updates = {
  rows: Notification[];
  /** how many notifications are unread */
  unread: number;
  /** the newest event the rows reflect, which Mark all as read takes */
  upTo: number;
  setRead: (n: Notification, read: boolean) => void;
  setDone: (n: Notification) => void;
  readAll: () => void;
};

/** The updates, kept current from the notices: each carries the
 *  notification it changed, and a change that names none loads them again. */
export function useUpdates(): Updates {
  const [rows, setRows] = useState<Notification[]>([]);
  const unread = useLive().unreadUpdates;
  const [upTo, setUpTo] = useState(0);
  const load = useCallback(() => {
    api
      .notifications({ tab: "all", limit: SHOWN })
      .then((list) => {
        setRows(list.notifications.filter(listed));
        setUpTo(list.up_to);
      })
      .catch(() => {});
  }, []);
  useEffect(load, [load]);
  useNotice((notice) => {
    if (notice.notification) {
      const changed = notice.notification;
      setRows((rows) => applyNotification(rows, changed));
      setUpTo((upTo) => Math.max(upTo, notice.event_id));
    } else if (notice.kind === "notification_changed" || notice.kind === "resync") {
      load();
    }
  });
  return {
    rows,
    unread,
    upTo,
    setRead: (n, read) => void api.setNotificationRead(n.id, read).catch(() => {}),
    setDone: (n) => void api.setNotificationDone(n.id, true).catch(() => {}),
    readAll: () => void api.readNotifications({ tab: "all", kind: ["item"] }, upTo).catch(() => {}),
  };
}

/** Where a notification's row leads: its work item. */
export const updatePath = (n: Notification) => (n.subject.kind === "item" ? itemPath(n.subject.item.key) : "/");

export function InboxUpdates({
  updates,
  first,
  focused,
  hover,
}: {
  updates: Updates;
  /** the first row's place in the order J and K walk */
  first: number;
  focused: number;
  hover: (index: number) => void;
}) {
  if (!updates.rows.length) return null;
  return (
    <section className="inbox-updates" aria-label="Updates">
      <h2 className="inbox-agents-title inbox-updates-title">
        Updates <span>{updates.rows.length}</span>
        {updates.unread > 0 ? <span className="inbox-updates-unread">{updates.unread} unread</span> : null}
      </h2>
      {updates.rows.map((n, i) => (
        <UpdateRow
          key={n.id}
          notification={n}
          here={first + i}
          focused={first + i === focused}
          hover={hover}
          updates={updates}
        />
      ))}
    </section>
  );
}

function UpdateRow({
  notification: n,
  here,
  focused,
  hover,
  updates,
}: {
  notification: Notification;
  here: number;
  focused: boolean;
  hover: (index: number) => void;
  updates: Updates;
}) {
  const item = n.subject.kind === "item" ? n.subject.item : null;
  const title = item?.title ?? "A work item";
  const button = (label: string, keys: string[], icon: ReactNode, act: () => void) => (
    <Tooltip label={label} keys={keys} side="top">
      <button
        type="button"
        className="bar-button"
        aria-label={`${label}: ${title}`}
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
      to={updatePath(n)}
      onClick={() => {
        if (!n.read) updates.setRead(n, true);
      }}
      className={`review-row update-row ${n.read ? "is-read" : "is-unread"} ${focused ? "is-focused" : ""}`}
      onMouseEnter={() => hover(here)}
      data-testid="update-row"
      data-row={here}
    >
      {n.read ? <span /> : <span className="review-row-marker" aria-label="Unread" />}
      {item ? <ItemStatusIcon status={item.status} /> : <span />}
      <span className="review-row-main">
        <span className="review-row-title truncate">
          {title}
          {item ? <span className="review-row-ref">{item.key}</span> : null}
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
      <span className="review-row-plugin">
        <ListChecks size={13} aria-hidden="true" />
        work item
      </span>
      <span className="review-row-end">
        <time className="review-row-time" dateTime={n.updated_at} title={stamp(n.updated_at)}>
          {age(n.updated_at)}
        </time>
        {n.read
          ? button("Mark as unread", ["U"], <Mail size={14} />, () => updates.setRead(n, false))
          : button("Mark as read", ["U"], <MailOpen size={14} />, () => updates.setRead(n, true))}
        {button("Mark as done", ["E"], <Check size={14} />, () => updates.setDone(n))}
      </span>
    </Link>
  );
}
