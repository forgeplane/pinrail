// The inbox's Agents section, above the reviews: each session whose agent
// waits on the person in its terminal, or finished a turn they have not
// seen, with the reason and the time.

import { Link } from "react-router";
import { api } from "../api/client";
import { age, stamp } from "../lib/format";
import { AGENT_ROWS, agentRows, type AgentRow } from "../lib/inboxAgents";
import { useLoad } from "../lib/useLoad";
import { STATE_LABEL, stateOf } from "../lib/sidebarSessions";
import { aboutSessions, useNoticeCount } from "../state/live";
import { AgentIcon } from "./AgentIcon";
import { sessionPath } from "./Thread";
import { useState } from "react";
import { sessionLabel } from "../lib/sessions";

/** How many sessions are fetched, more than the section shows, to tell
 *  whether there are more. */
const FETCHED = 50;

/** The section's rows, loaded again with each change to a session, and
 *  whether more wait than it shows. */
export function useInboxAgents(): { rows: AgentRow[]; more: boolean; now: number } {
  const changed = useNoticeCount(aboutSessions);
  const [now, setNow] = useState(() => Date.now());
  const sessions =
    useLoad(
      () =>
        api.sessions({ attention: "needs_you,finished", sort: "priority", limit: String(FETCHED) }).then((d) => {
          setNow(Date.now());
          return d.sessions;
        }),
      [changed],
    ).data ?? [];
  const rows = agentRows(sessions, now);
  return { rows: rows.slice(0, AGENT_ROWS), more: rows.length > AGENT_ROWS, now };
}

export function InboxAgents({
  rows,
  more,
  now,
  focused,
  hover,
}: {
  rows: AgentRow[];
  more: boolean;
  now: number;
  /** the row lit, by its place in the order J and K walk */
  focused: number;
  hover: (index: number) => void;
}) {
  if (!rows.length) return null;
  return (
    <section className="inbox-agents" aria-label="Agents">
      <h2 className="inbox-agents-title">Agents</h2>
      {rows.map(({ session, reason, since }, i) => {
        const state = stateOf(session, now);
        return (
          <Link
            key={`${session.agent}/${session.id}`}
            to={sessionPath(session)}
            className={`review-row agent-row ${i === focused ? "is-focused" : ""}`}
            onMouseEnter={() => hover(i)}
            data-testid="agent-row"
            data-row={i}
          >
            <span className={`agent-mark is-${state}`} role="img" aria-label={STATE_LABEL[state]}>
              <AgentIcon requestedBy={session.agent} size={15} />
            </span>
            <span className="review-row-main">
              <span className="review-row-title truncate">{sessionLabel(session)}</span>
              <span className="review-row-meta">
                <span>{reason}</span>
                {session.repo ? <span className="review-row-project">{session.repo}</span> : null}
              </span>
            </span>
            <span className="review-row-end">
              <time className="review-row-time" dateTime={since} title={stamp(since)}>
                {age(since, now)}
              </time>
            </span>
          </Link>
        );
      })}
      {more ? (
        <Link to="/sessions?attention=needs_you,finished" className="inbox-agents-more">
          Show all
        </Link>
      ) : null}
    </section>
  );
}
