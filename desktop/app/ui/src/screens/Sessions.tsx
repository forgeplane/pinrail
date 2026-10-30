// The agents' sessions that sent questions or reviews, those waiting for the
// person first, and one session's thread: in the side panel beside the list,
// or on a page of its own.

import { MessagesSquare, Search, SearchX } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { Link, useNavigate, useParams, type To } from "react-router";
import { api } from "../api/client";
import type { Session } from "../api/types";
import { AgentIcon } from "../components/AgentIcon";
import { EmptyState } from "../components/EmptyState";
import { Pager, pageOf, pageSizeOf } from "../components/Pager";
import { SidePanel } from "../components/SidePanel";
import { Thread, sessionPath, statusLine } from "../components/Thread";
import { age, stamp } from "../lib/format";
import { modalOpen } from "../lib/keys";
import { useUrlParams } from "../lib/url";
import { useLive } from "../state/live";
import { useSettings } from "../state/settings";
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
  const [params, updateParams, currentParams] = useUrlParams();
  const { settings: prefs } = useSettings();
  // as in the inbox, ?open= names the session shown beside the list, as
  // agent/id, when reviews open in the side panel
  const panelMode = prefs.inbox.open_in === "panel";
  const openKey = panelMode ? params.get("open") : null;
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

  const keyOf = (s: { agent: string; id: string }) => `${s.agent}/${s.id}`;
  /** Opens a session in the side panel, or closes the panel with null. */
  const openInPanel = useCallback(
    (key: string | null, options?: { replace?: boolean }) =>
      updateParams((next) => {
        if (key) next.set("open", key);
        else next.delete("open");
      }, options),
    [updateParams],
  );
  /** Opens a session: in the side panel, or on its own page. */
  const open = useCallback(
    (s: Session) => {
      if (panelMode) openInPanel(keyOf(s), { replace: currentParams().has("open") });
      else navigate(sessionPath(s));
    },
    [panelMode, openInPanel, currentParams, navigate],
  );
  /** The link of a session's row: the side panel, keeping the search, or its page. */
  const hrefFor = (s: Session): To => {
    if (!panelMode) return sessionPath(s);
    const next = new URLSearchParams(params);
    next.set("open", keyOf(s));
    return { pathname: "/sessions", search: `?${next}` };
  };
  /** A plain click on a session's link opens it in the side panel. */
  const openFromLink = (event: MouseEvent, s: Session) => {
    if (!panelMode || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    open(s);
  };
  const closePanel = useCallback(() => openInPanel(null), [openInPanel]);
  // the element that holds the list and the side panel
  const split = useRef<HTMLDivElement>(null);
  const opened = useMemo(() => {
    const at = openKey?.indexOf("/") ?? -1;
    return openKey && at > 0 ? { agent: openKey.slice(0, at), id: openKey.slice(at + 1) } : null;
  }, [openKey]);

  // the focus follows the session open in the side panel
  useEffect(() => {
    if (!openKey) return;
    const at = sessions.findIndex((s) => keyOf(s) === openKey);
    if (at >= 0) setFocused(at);
  }, [openKey, sessions]);

  // With a session open beside the list, J and K open the next and previous
  // session, wrapping around at the ends of the page, and Esc closes it.
  useEffect(() => {
    if (!openKey) return;
    const onWalk = (event: KeyboardEvent) => {
      if (modalOpen() || event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return;
      const el = event.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      if (event.key === "Escape") {
        event.preventDefault();
        closePanel();
        return;
      }
      if ((event.key !== "j" && event.key !== "k") || !sessions.length) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      const at = sessions.findIndex((s) => keyOf(s) === openKey);
      const next = at < 0 ? 0 : (at + (event.key === "j" ? 1 : sessions.length - 1)) % sessions.length;
      if (keyOf(sessions[next]) !== openKey) openInPanel(keyOf(sessions[next]), { replace: true });
    };
    window.addEventListener("keydown", onWalk, true);
    return () => window.removeEventListener("keydown", onWalk, true);
  }, [openKey, sessions, openInPanel, closePanel]);

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
      if (event.key === "Enter" && sessions[focused] && !openKey) open(sessions[focused]);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [sessions, focused, open, openKey]);

  return (
    <div className={`sessions ${opened ? "has-panel" : ""}`} ref={split}>
      <div className="sessions-side split-side">
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
                  <th className="sessions-col-total">In it</th>
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
                      className={`${i === focused ? "is-focused" : ""} ${waiting ? "is-waiting" : ""} ${keyOf(s) === openKey ? "is-open" : ""}`}
                      data-testid="session-row"
                      onMouseEnter={() => setFocused(i)}
                      onClick={(event) => {
                        if (!(event.target as Element).closest("a")) open(s);
                      }}
                    >
                      <td>
                        <Link to={hrefFor(s)} className="history-title with-icon" onClick={(e) => openFromLink(e, s)}>
                          <AgentIcon requestedBy={s.agent} size={13} /> {s.title ?? s.agent}
                        </Link>
                        <small className="mono" title={s.id}>
                          {s.title ? `${s.agent} · ${s.id}` : s.id}
                        </small>
                        {s.status && !(s.status.kind === "start" && s.title) ? (
                          <small
                            className={`session-status ${s.status.kind === "done" ? "is-done" : ""}`}
                            data-testid="session-status"
                          >
                            {statusLine(s.status)}
                          </small>
                        ) : null}
                      </td>
                      <td>{waiting ? <span className="thread-waiting">{contents(s.waiting)}</span> : null}</td>
                      <td className="dim sessions-col-total">{contents(s.total)}</td>
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
      {opened ? (
        <SidePanel split={split} label="Session">
          <div className="review-page thread-page in-panel">
            <Thread key={openKey} session={opened} inPanel onClose={closePanel} />
          </div>
        </SidePanel>
      ) : null}
    </div>
  );
}

/** A session's thread, at /sessions/:agent/:id. */
export function SessionScreen() {
  const { agent = "", id = "" } = useParams();
  // the task the agent works on names the session, once the thread has it
  const [title, setTitle] = useState<string | null>(null);
  const crumb = useMemo(
    () => (
      <span className="crumb" data-tauri-drag-region>
        <Link to="/sessions" className="crumb-root">
          Sessions
        </Link>
        <span className="crumb-sep">›</span>
        <span className="crumb-title">{title ?? agent}</span>
      </span>
    ),
    [agent, title],
  );
  useTopBar(useMemo(() => ({ crumb }), [crumb]));
  return (
    <div className="review-page thread-page">
      <Thread key={`${agent}/${id}`} session={{ agent, id }} onSession={(s) => setTitle(s.title)} />
    </div>
  );
}
