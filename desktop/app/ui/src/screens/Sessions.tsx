// The agents' sessions that sent questions or reviews, those waiting for the
// person first, and one session's thread: in the side panel beside the list,
// or on a page of its own.

import { FolderGit2, MessagesSquare, Search, SearchX } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { Link, useLocation, useNavigate, useParams, type To } from "react-router";
import { api } from "../api/client";
import type { Session } from "../api/types";
import { AgentIcon } from "../components/AgentIcon";
import { ArchiveButton, useToggleArchive } from "../components/ArchiveButton";
import { EmptyState } from "../components/EmptyState";
import { Pager, pageOf, pageSizeOf } from "../components/Pager";
import { GroupHeading } from "../components/GroupHeading";
import { FilterChips, ListMenuButtons, groupingDisplay, type Filter, type Toggle } from "../components/ListMenus";
import { groupKey, grouped, mixedGroups } from "../lib/grouping";
import { ProjectAvatar } from "../components/ProjectAvatar";
import { SidePanel } from "../components/SidePanel";
import { Thread, sessionPath } from "../components/Thread";
import { statusLine } from "../components/thread/messages";
import { age, stamp } from "../lib/format";
import { modalOpen, isTyping } from "../lib/keys";
import { scopeOf, scopeOptions, setScope, useUrlParams } from "../lib/url";
import { useLive, aboutSessions, useNoticeCount } from "../state/live";
import { useSettings } from "../state/settings";
import { useTopBar } from "../state/topbar";
import { ReviewView } from "./ReviewScreen";
import { useWindowKey } from "../lib/useWindowKey";

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
  const toggleArchive = useToggleArchive();
  const { settings: prefs, update } = useSettings();
  // as in the inbox, ?open= names the session shown beside the list, as
  // agent/id, when reviews open in the side panel
  const panelMode = prefs.inbox.open_in === "panel";
  const openKey = panelMode ? params.get("open") : null;
  const [q, setQ] = useState(params.get("q") ?? "");
  const query = params.get("q") ?? "";
  const repo = params.get("repo") ?? "";
  const project = params.get("project") ?? "";
  const scope = scopeOf(params);
  // with the archived sessions among the others
  const withArchived = params.get("archived") === "1";
  const filtered = !!(query || repo || project);
  const page = pageOf(params.get("page"));
  const size = pageSizeOf(params.get("per"));
  const [data, setData] = useState<Awaited<ReturnType<typeof api.sessions>> | null>(null);
  const [focused, setFocused] = useState(0);

  const [debounced, setDebounced] = useState(query);
  useEffect(() => {
    const t = window.setTimeout(() => setDebounced(query), SEARCH_DELAY_MS);
    return () => window.clearTimeout(t);
  }, [query]);
  // anything sent, answered or withdrawn moves the sessions
  const sessionNotices = useNoticeCount(aboutSessions);
  useEffect(() => {
    let cancelled = false;
    api
      .sessions({
        q: debounced,
        repo,
        project,
        include_archived: withArchived ? "true" : undefined,
        limit: String(size),
        offset: String((page - 1) * size),
      })
      .then((d) => !cancelled && setData(d))
      .catch(() => !cancelled && setData(null));
    return () => {
      cancelled = true;
    };
  }, [debounced, repo, project, withArchived, page, size, sessionNotices]);

  // grouped by project or by repository within the page shown, or one
  // list; J, K and Enter walk the rows in the order they are drawn
  const layout = prefs.sessions.layout;
  const groups = useMemo(() => {
    const fetched = data?.sessions ?? [];
    return layout === "list" ? null : grouped(fetched, (s) => groupKey(layout, s.repo, s.project));
  }, [data, layout]);
  const sessions = useMemo(
    () => (groups ? groups.flatMap(([, items]) => items) : (data?.sessions ?? [])),
    [groups, data],
  );
  const mixed = useMemo(() => (groups ? mixedGroups(groups, (s) => s.repo) : new Set<string>()), [groups]);
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
  useWindowKey(
    (event) => {
      if (modalOpen() || event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return;
      const el = event.target as HTMLElement | null;
      if (isTyping(el)) return;
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
    },
    { capture: true, enabled: !!openKey },
  );

  // the focused row stays in view as J and K move it
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    list.current?.children[focused]?.scrollIntoView({ block: "nearest" });
  }, [focused]);

  // the keys of the other lists: j and k move, Enter opens, / searches
  useWindowKey((event) => {
    if (event.metaKey || event.ctrlKey || event.altKey || modalOpen()) return;
    const el = event.target as HTMLElement | null;
    const typing = isTyping(el);
    if (event.key === "/" && !typing) {
      event.preventDefault();
      document.querySelector<HTMLInputElement>(".sessions input[type=search]")?.focus();
      return;
    }
    if (typing) return;
    if (event.key === "j") setFocused((f) => Math.min(f + 1, sessions.length - 1));
    if (event.key === "k") setFocused((f) => Math.max(f - 1, 0));
    if (event.key === "Enter" && sessions[focused] && !openKey) open(sessions[focused]);
    // E archives the focused row; with a session open, the thread takes it
    if ((event.key === "e" || event.key === "E") && !event.shiftKey && sessions[focused] && !openKey)
      toggleArchive(sessions[focused]);
  });

  /** A session's row; `i` is its place in the order J and K walk, and
   *  `group` the group it is drawn in, if any. */
  const row = (s: Session, i: number, group?: string) => {
    const waiting = s.waiting.questions + s.waiting.reviews;
    // the newest status, unless it is the start that titles the session
    const status = s.status && !(s.status.kind === "start" && s.title) ? s.status : null;
    return (
      <Link
        key={keyOf(s)}
        to={hrefFor(s)}
        onClick={(event) => openFromLink(event, s)}
        className={`review-row session-row ${waiting ? "is-waiting" : ""} ${s.archived_at ? "is-archived" : ""} ${i === focused ? "is-focused" : ""} ${keyOf(s) === openKey ? "is-open" : ""}`}
        aria-current={keyOf(s) === openKey ? "true" : undefined}
        onMouseEnter={() => setFocused(i)}
        data-testid="session-row"
      >
        <AgentIcon requestedBy={s.agent} size={15} />
        <span className="review-row-main">
          <span className="review-row-title">
            {s.title ?? s.agent}
            {s.ref ? <span className="review-row-ref">{s.ref}</span> : null}
            {s.archived_at ? <span className="archived-chip">Archived</span> : null}
          </span>
          <span className="review-row-meta">
            {s.title ? <span>{s.agent}</span> : null}
            {s.repo && (!group || mixed.has(group)) ? <span className="review-row-project">{s.repo}</span> : null}
            <span className="session-row-id mono" title={s.id}>
              {s.id}
            </span>
            {status ? (
              <span
                className={`session-status ${status.kind === "done" ? "is-done" : ""}`}
                data-testid="session-status"
              >
                {statusLine(status)}
              </span>
            ) : null}
          </span>
        </span>
        <span className="session-row-waiting">
          {waiting ? <span className="asks-chip">{contents(s.waiting)} waiting</span> : null}
        </span>
        <span className="session-row-total">{contents(s.total)}</span>
        <span className="review-row-end">
          <time className="review-row-time" dateTime={s.last_at} title={stamp(s.last_at)}>
            {age(s.last_at)}
          </time>
          <ArchiveButton session={s} inRow />
        </span>
      </Link>
    );
  };

  // the filters, in the Filter menu and as chips while they are on
  const clearFilters = () =>
    updateParams(
      (next) => {
        for (const key of ["q", "repo", "project", "archived", "page"]) next.delete(key);
      },
      { replace: true },
    );
  const filters: Filter[] = [
    {
      key: "scope",
      label: "Project or repository",
      value: scope,
      off: "",
      searchable: true,
      choices: scopeOptions(live.namedProjects, data?.facets.repos ?? [], !!data?.facets.unassigned, scope).map(
        (o) => ({
          value: o.value,
          label: o.label,
          mark: o.project ? <ProjectAvatar project={o.project} size={14} /> : <FolderGit2 size={13} />,
        }),
      ),
      onChange: (v) =>
        updateParams(
          (next) => {
            setScope(next, v);
            next.delete("page");
          },
          { replace: true },
        ),
    },
  ];
  const toggles: Toggle[] = [
    {
      key: "archived",
      label: "Show archived",
      on: withArchived,
      onChange: (on) => setParam("archived", on ? "1" : null),
    },
  ];

  return (
    <div className={`sessions ${opened ? "has-panel" : ""}`} ref={split}>
      <div className="sessions-side split-side">
        <header className="page-head">
          <h1>Sessions</h1>
          <div className="inbox-controls">
            <label className="search-field">
              <Search size={14} aria-hidden="true" />
              <input
                autoCorrect="off"
                autoCapitalize="off"
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
            <ListMenuButtons
              label="sessions"
              filters={filters}
              toggles={toggles}
              onReset={clearFilters}
              displays={[
                groupingDisplay(layout, (next) => {
                  void update({ sessions: { layout: next } });
                  setFocused(0);
                }),
              ]}
            />
          </div>
        </header>
        <FilterChips filters={filters} toggles={toggles} onReset={clearFilters} />
        {!data ? null : total === 0 ? (
          <EmptyState
            title={filtered ? "No matching sessions" : "Agents' sessions belong here"}
            icon={filtered ? <SearchX size={28} strokeWidth={1.5} /> : <MessagesSquare size={28} strokeWidth={1.5} />}
          >
            {filtered
              ? "Try a different search or clear the filters."
              : "Each agent session that asks you a question or sends a review appears here, with everything it sent."}
          </EmptyState>
        ) : (
          <>
            {groups ? (
              <div className="inbox-groups" ref={list}>
                {groups.map(([key, items]) => (
                  <details key={key || "none"} className="inbox-repo" open data-testid="inbox-group">
                    <summary data-testid="inbox-group-heading">
                      <GroupHeading groupKey={key} project={items[0].project} count={items.length} />
                    </summary>
                    {items.map((s) => row(s, sessions.indexOf(s), key))}
                  </details>
                ))}
              </div>
            ) : (
              <div className="inbox-list" ref={list}>
                {sessions.map((s, i) => row(s, i))}
              </div>
            )}
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
          </>
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
  // the stage the command palette opened the thread at
  const stage = (useLocation().state as { stage?: number } | null)?.stage;
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
      <Thread
        key={`${agent}/${id}/${stage ?? ""}`}
        session={{ agent, id }}
        stage={stage}
        onSession={(s) => setTitle(s.title)}
        reviewPanel={({ id: review, linkTo, onEnded, onClose }) => (
          <ReviewView
            key={review}
            id={review}
            inPanel
            besideThread
            linkTo={linkTo}
            onEnded={onEnded}
            onClose={onClose}
          />
        )}
      />
    </div>
  );
}
