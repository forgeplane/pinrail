// A notification's row unfolded: its new events, one line each, grouped by
// who caused them, with the earlier ones on request, and links to open the
// subject and mark the notification read.

import { ChevronRight } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, type To } from "react-router";
import { api } from "../api/client";
import type { Notice, Notification } from "../api/types";
import { age, stamp } from "../lib/format";
import { actorName, eventLine } from "../state/inboxUpdates";
import { AgentIcon } from "./AgentIcon";

export function NotificationEvents({
  notification: n,
  href,
  onRead,
}: {
  notification: Notification;
  href: To;
  onRead: () => void;
}) {
  const [events, setEvents] = useState<Notice[] | null>(null);
  // the new events, or with the earlier ones every event of the subject
  const [all, setAll] = useState(false);
  useEffect(() => {
    let current = true;
    api
      .notificationEvents(n.id, all ? 0 : n.read_event)
      .then((answer) => current && setEvents(answer.events))
      .catch(() => {});
    return () => {
      current = false;
    };
  }, [n.id, n.read_event, n.last_event, all]);

  const lines = (events ?? []).flatMap((e) => {
    const line = eventLine(e);
    return line ? [{ event: e, line, who: actorName(e) }] : [];
  });
  // consecutive events by one actor go under one name
  const groups: { who: string; agent: boolean; lines: typeof lines }[] = [];
  for (const l of lines) {
    const last = groups[groups.length - 1];
    if (last?.who === l.who) last.lines.push(l);
    else groups.push({ who: l.who, agent: l.event.actor?.kind === "agent", lines: [l] });
  }
  const label =
    n.subject.kind === "item" ? "Open work item" : n.subject.kind === "session" ? "Open session" : "Open review";
  return (
    <div className="notification-events" data-testid="notification-events">
      {!all && n.read_event > 0 ? (
        <button type="button" className="notification-events-earlier" onClick={() => setAll(true)}>
          <ChevronRight size={12} aria-hidden="true" /> Show earlier events
        </button>
      ) : null}
      {groups.map((g, i) => (
        <div key={i} className="notification-events-group">
          <span className="notification-events-who">
            {g.agent ? <AgentIcon requestedBy={g.who} size={13} /> : null}
            {g.who}
          </span>
          {g.lines.map(({ event, line }) => (
            <span key={event.event_id} className="notification-event" data-testid="notification-event">
              <span className="notification-event-text">
                {line.text}
                {line.detail ? <span className="notification-event-detail"> · {line.detail}</span> : null}
              </span>
              {event.at ? (
                <time dateTime={event.at} title={stamp(event.at)}>
                  {age(event.at)}
                </time>
              ) : null}
            </span>
          ))}
        </div>
      ))}
      <div className="notification-events-actions">
        <Link to={href}>{label}</Link>
        {n.read ? null : (
          <>
            <span>·</span>
            <button type="button" onClick={onRead}>
              Mark as read
            </button>
          </>
        )}
      </div>
    </div>
  );
}
