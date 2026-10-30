// The sidebar's sessions: every session that is not archived, grouped and
// ordered as the heading's menu says, with what waits for the person under
// the session that sent it. The grouping, ordering and filtering are in
// lib/sidebarSessions; this draws them and keeps the choices in the
// settings.

import { ChevronDown, ChevronRight, FolderGit2, SlidersHorizontal } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Link, NavLink } from "react-router";
import { api } from "../api/client";
import type { Review, Session } from "../api/types";
import { age, stamp } from "../lib/format";
import { type Choices, type Group, type Row, HEADINGS, arrange } from "../lib/sidebarSessions";
import { NO_PROJECT } from "../lib/url";
import { useLive } from "../state/live";
import { useSettings } from "../state/settings";
import { AgentIcon } from "./AgentIcon";
import { ReviewIcon } from "./ReviewIcon";
import { SidebarSessionsMenu } from "./SidebarSessionsMenu";
import { sessionPath } from "./Thread";
import { Tooltip } from "./Tooltip";

/** How often the working marks are checked against the time. */
const TICK_MS = 30_000;
/** At most this many sessions are fetched for the sidebar. */
const FETCHED = 200;

const STATE_LABEL = { working: "Working", quiet: "Quiet", done: "Done" } as const;

export function SidebarSessions({ shown }: { shown: string | null }) {
  const live = useLive();
  const { settings, update } = useSettings();
  const choices = settings.sidebar.sessions;
  const [sessions, setSessions] = useState<Session[]>([]);
  const [now, setNow] = useState(() => Date.now());
  const [menu, setMenu] = useState(false);
  const button = useRef<HTMLButtonElement>(null);

  // a status may rename a session or mark it done, and archiving one moves
  // it in or out
  const reported =
    live.lastNotice && ["session_status", "session_archived", "session_unarchived"].includes(live.lastNotice.kind)
      ? live.lastNotice.event_id
      : null;
  useEffect(() => {
    let cancelled = false;
    api
      .sessions({ limit: String(FETCHED), include_archived: choices.archived ? "true" : undefined })
      .then((d) => !cancelled && setSessions(d.sessions))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [live.pending, reported, choices.archived]);
  // a session stops working once its last status is old enough
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), TICK_MS);
    return () => window.clearInterval(timer);
  }, []);
  useEffect(() => setNow(Date.now()), [sessions]);

  const groups = useMemo(() => arrange(sessions, live.pending, choices, now), [sessions, live.pending, choices, now]);
  const agents = useMemo(() => [...new Set(sessions.map((s) => s.agent))].sort(), [sessions]);
  const repos = useMemo(
    () =>
      [...new Set(sessions.map((s) => s.repo ?? NO_PROJECT))].sort((a, b) =>
        a === NO_PROJECT ? 1 : b === NO_PROJECT ? -1 : a.localeCompare(b),
      ),
    [sessions],
  );

  const change = (patch: Partial<Choices & { collapsed: string[] }>) => update({ sidebar: { sessions: patch } });
  const keyOf = (group: Group) => `${choices.grouping}:${group.key}`;
  const collapsed = new Set(choices.collapsed);
  const collapsedAll = groups.length > 0 && groups.every((g) => collapsed.has(keyOf(g)));
  const toggleGroup = (group: Group) => {
    const key = keyOf(group);
    change({
      collapsed: collapsed.has(key) ? choices.collapsed.filter((k) => k !== key) : [...choices.collapsed, key],
    });
  };
  const grouped = choices.grouping !== "none";

  /**
   * A review on its own page, with the inbox as the way back. A question
   * from a session opens on its session's page, where it is answered: the
   * session's row then goes to the same page and keeps the thread drawn.
   */
  const linkOf = (review: Review) =>
    review.kind === "question" && review.requested_by && review.session
      ? sessionPath({ agent: review.requested_by, id: review.session })
      : `/reviews/${review.id}`;
  const reviewLink = (review: Review) => (
    <Link
      key={review.id}
      to={linkOf(review)}
      className={`sidebar-review sidebar-child ${review.kind === "question" ? "is-question" : ""} ${shown === review.id ? "active" : ""}`}
      aria-current={shown === review.id ? "page" : undefined}
      data-testid="waiting-review"
    >
      <ReviewIcon review={review} size={13} strokeWidth={1.75} />
      <span className="sidebar-review-title">{review.title}</span>
    </Link>
  );

  const sessionRow = (row: Row) => {
    const s = row.session;
    const details = choices.show_details
      ? choices.details
          .map((d) =>
            d === "activity"
              ? `${age(s.last_at, now)} ago`
              : d === "status"
                ? s.status?.kind === "step"
                  ? s.status.text
                  : null
                : d === "branch"
                  ? s.ref
                  : s.agent,
          )
          .filter(Boolean)
          .join(" · ")
      : "";
    const tip = (
      <span className="sidebar-session-tip">
        <span>
          {STATE_LABEL[row.state]} · {age(s.status?.at ?? s.last_at, now)} ago
        </span>
        {s.status?.text ? <span className="faint">{s.status.text}</span> : null}
        <span className="faint">{[s.agent, s.ref].filter(Boolean).join(" · ")}</span>
      </span>
    );
    return (
      <div key={`${s.agent}/${s.id}`} className="sidebar-session-item">
        <Tooltip label={tip} side="top">
          <NavLink
            to={sessionPath(s)}
            className={`sidebar-session is-${row.state} ${grouped ? "" : "is-flat"}`}
            data-testid="sidebar-session"
            data-state={row.state}
          >
            <span className={`agent-mark is-${row.state}`} aria-label={STATE_LABEL[row.state]}>
              <AgentIcon requestedBy={s.agent} size={14} />
            </span>
            <span className="sidebar-session-text">
              <span className="sidebar-review-title">{s.title ?? s.agent}</span>
              {details ? (
                <span className="sidebar-session-details" title={stamp(s.last_at)}>
                  {details}
                </span>
              ) : null}
            </span>
            {row.waiting > 0 ? (
              <span className="sidebar-badge" data-testid="sidebar-session-waiting">
                {row.waiting}
              </span>
            ) : null}
          </NavLink>
        </Tooltip>
        {choices.show_reviews ? row.reviews.map(reviewLink) : null}
      </div>
    );
  };

  const groupHead = (group: Group) => {
    const open = !collapsed.has(keyOf(group));
    const Chevron = open ? ChevronDown : ChevronRight;
    return (
      <div className="sidebar-group-head" data-testid="sidebar-group">
        <button
          type="button"
          className="sidebar-group-toggle"
          aria-expanded={open}
          aria-label={`${open ? "Collapse" : "Expand"} ${group.label}`}
          onClick={() => toggleGroup(group)}
        >
          <Chevron size={12} />
        </button>
        {choices.grouping === "repository" ? (
          <NavLink to={`/?repo=${encodeURIComponent(group.key || NO_PROJECT)}`} className="sidebar-group-name">
            <FolderGit2 size={14} strokeWidth={1.75} className={group.key ? "" : "is-faint"} />
            <span className="sidebar-review-title" data-testid="sidebar-group-name">
              {group.label}
            </span>
          </NavLink>
        ) : (
          <button type="button" className="sidebar-group-name" onClick={() => toggleGroup(group)}>
            <span className="sidebar-review-title" data-testid="sidebar-group-name">
              {group.label}
            </span>
          </button>
        )}
        {group.waiting > 0 ? <span className="nav-count">{group.waiting}</span> : null}
      </div>
    );
  };

  return (
    <section className="sidebar-sessions" aria-label={HEADINGS[choices.grouping]}>
      <h2>
        {HEADINGS[choices.grouping]}
        <Tooltip label="Grouping, ordering and filters" side="bottom">
          <button
            ref={button}
            type="button"
            className={`bar-button sidebar-sessions-button ${menu ? "is-on" : ""}`}
            aria-label="Sessions options"
            aria-haspopup="menu"
            aria-expanded={menu}
            onClick={() => setMenu((m) => !m)}
          >
            <SlidersHorizontal size={13} />
          </button>
        </Tooltip>
      </h2>
      {groups.length === 0 ? <p className="sidebar-empty">No sessions yet</p> : null}
      {groups.map((group) => {
        const open = !grouped || !collapsed.has(keyOf(group));
        return (
          <div key={group.key || "none"} className="sidebar-group">
            {grouped ? groupHead(group) : null}
            {open ? (
              <>
                {group.loose.map(reviewLink)}
                {group.rows.map(sessionRow)}
              </>
            ) : null}
          </div>
        );
      })}
      {menu && button.current ? (
        <SidebarSessionsMenu
          anchor={button.current}
          choices={choices}
          agents={agents}
          repos={repos}
          collapsedAll={collapsedAll}
          onChange={change}
          onCollapseAll={(collapse) => change({ collapsed: collapse ? groups.map(keyOf) : [] })}
          onClose={() => setMenu(false)}
        />
      ) : null}
    </section>
  );
}
