// The agents' sessions that sent questions or reviews, those waiting for the
// person first, and one session's thread on a page of its own.

import { MessagesSquare, Search, SearchX } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { api } from "../api/client";
import type { Session } from "../api/types";
import { AgentIcon } from "../components/AgentIcon";
import { EmptyState } from "../components/EmptyState";
import { Pager, pageOf, pageSizeOf } from "../components/Pager";
import { Thread, sessionPath } from "../components/Thread";
import { age, stamp } from "../lib/format";
import { modalOpen } from "../lib/keys";
import { useUrlParams } from "../lib/url";
import { useLive } from "../state/live";
import { useTopBar } from "../state/topbar";

/** How long the search waits for typing to pause before it asks the server. */
const SEARCH_DELAY_MS = 250;

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** What a session holds, as "2 questions · 1 review". */
const contents = (counts: { questions: number; reviews: number }) =>
  [
    counts.questions ? plural(counts.questions, "question", "questions") : null,
    counts.reviews ? plural(counts.reviews, "review", "reviews") : null,
  ]
    .filter(Boolean)
    .join(" · ");

export function Sessions() {
  const live = useLive();
  const navigate = useNavigate();
  const [params, updateParams] = useUrlParams();
  const [q, setQ] = useState(params.get("q") ?? "");
  const query = params.get("q") ?? "";
  const page = pageOf(params.get("page"));
  const size = pageSizeOf(params.get("per"));
  const [data, setData] = useState<{ sessions: Session[]; total: number } | null>(null);
  const [focused, setFocused] = useState(0);

  const [debounced, setDebounced] = useState(query);
  useEffect(() => {
    const t = window.setTimeout(() => setDebounced(query), SEARCH_DELAY_MS);
    return () => window.clearTimeout(t);
  }, [query]);
  // anything sent, answered or withdrawn moves the sessions
  useEffect(() => {
    let cancelled = false;
    api
      .sessions({ q: debounced, limit: String(size), offset: String((page - 1) * size) })
      .then((d) => !cancelled && setData(d))
      .catch(() => !cancelled && setData(null));
    return () => {
      cancelled = true;
    };
  }, [debounced, page, size, live.lastNotice]);

  const sessions = useMemo(() => data?.sessions ?? [], [data]);
  const total = data?.total ?? 0;
  const setParam = (key: string, value: string | null) =>
    updateParams(
      (next) => {
        if (value) next.set(key, value);
        else next.delete(key);
        if (key !== "page") next.delete("page");
      },
      { replace: true },
    );

  // the keys of the other lists: j and k move, Enter opens, / searches
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || modalOpen()) return;
      const el = event.target as HTMLElement | null;
      const typing = !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT");
      if (event.key === "/" && !typing) {
        event.preventDefault();
        document.querySelector<HTMLInputElement>(".sessions input[type=search]")?.focus();
        return;
      }
      if (typing) return;
      if (event.key === "j") setFocused((f) => Math.min(f + 1, sessions.length - 1));
      if (event.key === "k") setFocused((f) => Math.max(f - 1, 0));
      if (event.key === "Enter" && sessions[focused]) navigate(sessionPath(sessions[focused]));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [sessions, focused, navigate]);

  return (
    <div className="sessions">
      <header className="page-head">
        <h1>Sessions</h1>
        <div className="inbox-controls">
          <label className="search-field">
            <Search size={14} aria-hidden="true" />
            <input
              type="search"
              autoComplete="off"
              spellCheck={false}
              placeholder="Search sessions…"
              aria-label="Search sessions"
              value={q}
              onChange={(e) => {
                setQ(e.target.value);
                setParam("q", e.target.value);
              }}
            />
          </label>
        </div>
      </header>
      {!data ? null : total === 0 ? (
        <EmptyState
          title={query ? "No matching sessions" : "Agents' sessions belong here"}
          icon={query ? <SearchX size={28} strokeWidth={1.5} /> : <MessagesSquare size={28} strokeWidth={1.5} />}
        >
          {query
            ? "Nothing matches this search."
            : "Each agent session that asks you a question or sends a review appears here, with everything it sent."}
        </EmptyState>
      ) : (
        <div className="history-table-wrap">
          <table className="history-table sessions-table">
            <thead>
              <tr>
                <th>Session</th>
                <th>Waiting</th>
                <th>In it</th>
                <th className="history-col-repo">Project</th>
                <th className="history-col-recorded">Last activity</th>
              </tr>
            </thead>
            <tbody>
              {sessions.map((s, i) => {
                const waiting = s.waiting.questions + s.waiting.reviews;
                return (
                  <tr
                    key={`${s.agent}/${s.id}`}
                    className={`${i === focused ? "is-focused" : ""} ${waiting ? "is-waiting" : ""}`}
                    data-testid="session-row"
                    onMouseEnter={() => setFocused(i)}
                    onClick={(event) => {
                      if (!(event.target as Element).closest("a")) navigate(sessionPath(s));
                    }}
                  >
                    <td>
                      <Link to={sessionPath(s)} className="history-title with-icon">
                        <AgentIcon requestedBy={s.agent} size={13} /> {s.agent}
                      </Link>
                      <small className="mono" title={s.id}>
                        {s.id}
                      </small>
                    </td>
                    <td>{waiting ? <span className="thread-waiting">{contents(s.waiting)}</span> : null}</td>
                    <td className="dim">{contents(s.total)}</td>
                    <td className="history-col-repo">
                      {s.repo}
                      {s.ref ? <small className="mono">{s.ref}</small> : null}
                    </td>
                    <td className="history-col-recorded" title={stamp(s.last_at)}>
                      {age(s.last_at)} ago
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <Pager
            page={page}
            size={size}
            total={total}
            label="Sessions"
            onPage={(n) => {
              const value = typeof n === "function" ? n(page) : n;
              setParam("page", value > 1 ? String(value) : null);
            }}
            onSize={(n) => setParam("per", String(n))}
          />
        </div>
      )}
    </div>
  );
}

/** A session's thread, at /sessions/:agent/:id. */
export function SessionScreen() {
  const { agent = "", id = "" } = useParams();
  const crumb = useMemo(
    () => (
      <span className="crumb" data-tauri-drag-region>
        <Link to="/sessions" className="crumb-root">
          Sessions
        </Link>
        <span className="crumb-sep">›</span>
        <span className="crumb-title">{agent}</span>
      </span>
    ),
    [agent],
  );
  useTopBar(useMemo(() => ({ crumb }), [crumb]));
  return (
    <div className="review-page">
      <Thread key={`${agent}/${id}`} session={{ agent, id }} />
    </div>
  );
}
