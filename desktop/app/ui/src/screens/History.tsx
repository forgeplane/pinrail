import { Archive, Blocks, CircleDot, FolderGit2, Search, SearchX } from "lucide-react";
import { type MouseEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, type To } from "react-router";
import { api } from "../api/client";
import type { ReviewListing } from "../api/types";
import { AgentIcon } from "../components/AgentIcon";
import { FilesCount, OutcomeBadge, SummaryCounts } from "../components/Badges";
import { EmptyState } from "../components/EmptyState";
import { Pager, pageOf, pageSizeOf } from "../components/Pager";
import { PluginIcon } from "../components/PluginIcon";
import { Select } from "../components/Select";
import { Tooltip } from "../components/Tooltip";
import { settledAt, stamp } from "../lib/format";
import { clearAll, useUrlParams } from "../lib/url";
import { useLive } from "../state/live";
import { NO_PROJECT } from "../lib/url";
import { modalOpen } from "../lib/keys";
import { SourceBadge } from "../components/SourceBadge";
import { SidePanel } from "../components/SidePanel";
import { useSettings } from "../state/settings";

const ENDED = "decided,withdrawn,discarded,expired";
/** How long the search waits for typing to pause before it asks the server. */
const SEARCH_DELAY_MS = 250;

export function History() {
  const live = useLive();
  const [params, updateParams, currentParams] = useUrlParams();
  const navigate = useNavigate();
  const { settings: prefs } = useSettings();
  const [data, setData] = useState<ReviewListing | null>(null);
  const [focused, setFocused] = useState(0);
  const search = useRef<HTMLInputElement>(null);
  // While the keyboard moves the focus the list scrolls under a still
  // pointer, and the row that slides under it must not take the focus
  // back; the next real movement of the pointer hands it over again.
  const keyboard = useRef(false);
  useEffect(() => {
    const onMove = () => {
      keyboard.current = false;
    };
    window.addEventListener("mousemove", onMove);
    return () => window.removeEventListener("mousemove", onMove);
  }, []);
  const status = params.get("status") ?? "";
  const plugin = params.get("plugin") ?? "";
  const repo = params.get("repo") ?? "";
  // the search box keeps its own text; see the inbox for why
  const [q, setQ] = useState(params.get("q") ?? "");
  const query = params.get("q") ?? "";
  const page = pageOf(params.get("page"));
  const size = pageSizeOf(params.get("per"));
  const filtered = !!(q || status || plugin || repo);
  // with the side panel, as the inbox opens reviews, ?open= names the review shown beside the list
  const panelMode = prefs.inbox.open_in === "panel";
  const openId = panelMode ? params.get("open") : null;

  // The server filters and pages: the history can be far longer than any
  // one fetch. The search waits for typing to pause.
  const [debounced, setDebounced] = useState(query);
  useEffect(() => {
    const t = window.setTimeout(() => setDebounced(query), SEARCH_DELAY_MS);
    return () => window.clearTimeout(t);
  }, [query]);
  useEffect(() => {
    let cancelled = false;
    api
      .listReviews({
        status: status || ENDED,
        include_revised: "true",
        q: debounced,
        plugin,
        repo,
        offset: String((page - 1) * size),
        limit: String(size),
        include: "facets",
      })
      .then((p) => !cancelled && setData(p))
      .catch(() => !cancelled && setData(null));
    return () => {
      cancelled = true;
    };
  }, [status, debounced, plugin, repo, page, size, live.historyVersion]);

  const reviews = useMemo(() => data?.reviews ?? [], [data]);
  const total = data?.total ?? 0;
  const pages = Math.max(1, Math.ceil(total / size));

  // a page past the end, after a filter or a sweep shrank the list: the last page
  useEffect(() => {
    if (data && total > 0 && reviews.length === 0 && page > 1) setPage(Math.ceil(total / size));
  }, [data]); // eslint-disable-line react-hooks/exhaustive-deps -- only when a page arrives, not when the page number changes

  /** Changing a filter starts again from the first page. */
  const setFilter = (key: string, value: string) => {
    updateParams(
      (next) => {
        if (value) next.set(key, value);
        else next.delete(key);
        next.delete("page");
      },
      { replace: true },
    );
  };
  /** A page number, or a step from the page the URL is on. */
  const setPage = (n: number | ((current: number) => number)) => {
    updateParams((next) => {
      const now = Math.min(pageOf(next.get("page")), pages);
      const wanted = typeof n === "function" ? n(now) : n;
      const value = Math.min(Math.max(1, wanted), pages);
      if (value > 1) next.set("page", String(value));
      else next.delete("page");
    });
    setFocused(0);
    document.querySelector(".history-table-wrap")?.scrollTo({ top: 0 });
    window.scrollTo({ top: 0 });
  };
  const setSize = (n: number) => {
    updateParams(
      (next) => {
        next.set("per", String(n));
        next.delete("page");
      },
      { replace: true },
    );
  };

  const plugins = data?.facets?.plugins ?? [];

  /** The link of a review's row: the side panel, keeping the filters, or its page. */
  const hrefFor = useCallback(
    (id: string): To => {
      if (!panelMode) return `/reviews/${id}`;
      const next = new URLSearchParams(params);
      next.set("open", id);
      return { pathname: "/history", search: `?${next}` };
    },
    [panelMode, params],
  );
  /** Opens a review in the side panel, or closes the panel with null. */
  const openInPanel = useCallback(
    (id: string | null, options?: { replace?: boolean }) =>
      updateParams((next) => {
        if (id) next.set("open", id);
        else next.delete("open");
      }, options),
    [updateParams],
  );
  const closePanel = useCallback(() => openInPanel(null), [openInPanel]);
  /**
   * A plain click on a review's link in the panel mode: opens it in the side
   * panel, replacing the history entry when the panel is open already. The
   * address decides that, as a second click may come before the render that
   * shows the first.
   */
  const openFromLink = (event: MouseEvent, id: string) => {
    if (!panelMode || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    openInPanel(id, { replace: currentParams().has("open") });
  };
  /** Opens a review: in the side panel, or on its own page. */
  const open = (id: string) => {
    if (panelMode) openInPanel(id, { replace: currentParams().has("open") });
    else navigate(`/reviews/${id}`, { state: { from: "history" } });
  };
  // the element that holds the list and the side panel
  const split = useRef<HTMLDivElement>(null);

  // The focus follows the review open in the side panel. Opened from the
  // keyboard, the list may scroll under a still pointer, and the row that
  // slides under it must not take the focus back.
  useEffect(() => {
    if (!openId) return;
    const at = reviews.findIndex((r) => r.id === openId);
    if (at < 0) return;
    keyboard.current = true;
    setFocused(at);
  }, [openId, reviews]);

  // With a review open beside the list, J and K open the next and previous
  // review, from the list or from the view, which forwards them, and wrap
  // around at the ends of the page.
  useEffect(() => {
    const onWalk = (event: KeyboardEvent) => {
      if (!openId || modalOpen() || event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return;
      if (event.key !== "j" && event.key !== "k") return;
      const el = event.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      if (!reviews.length) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      const at = reviews.findIndex((r) => r.id === openId);
      const next = at < 0 ? 0 : (at + (event.key === "j" ? 1 : reviews.length - 1)) % reviews.length;
      if (reviews[next].id !== openId) openInPanel(reviews[next].id, { replace: true });
    };
    window.addEventListener("keydown", onWalk, true);
    return () => window.removeEventListener("keydown", onWalk, true);
  }, [openId, reviews, openInPanel]);

  useEffect(() => {
    setFocused((f) => Math.min(f, Math.max(0, reviews.length - 1)));
  }, [reviews.length]);

  // the same keys as the inbox: j/k move, enter opens, / searches
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || modalOpen()) return;
      const el = event.target as HTMLElement | null;
      const typing = !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT");
      if (event.key === "/" && !typing) {
        event.preventDefault();
        search.current?.focus();
        return;
      }
      if (typing) {
        if (event.key === "Escape") el.blur();
        // Enter leaves the field for the results, so J, K and Enter work on them
        if (event.key === "Enter" && el === search.current) {
          event.preventDefault();
          el.blur();
          setFocused(0);
        }
        return;
      }
      if (event.key === "j" || event.key === "k") keyboard.current = true;
      if (event.key === "j") setFocused((f) => Math.min(f + 1, reviews.length - 1));
      if (event.key === "k") setFocused((f) => Math.max(f - 1, 0));
      if (event.key === "Enter" && reviews[focused] && !openId) open(reviews[focused].id);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [reviews, focused, navigate, openId, panelMode]); // eslint-disable-line react-hooks/exhaustive-deps -- open follows panelMode and openId

  useEffect(() => {
    document.querySelector<HTMLElement>(`[data-history-row="${focused}"]`)?.scrollIntoView({ block: "nearest" });
  }, [focused]);
  const repos = data?.facets?.repos ?? [];

  return (
    <div className={`history ${openId ? "has-panel" : ""}`} ref={split}>
      <div className="history-side split-side">
        <header className="page-head">
          <h1>History</h1>
          <div className="inbox-controls">
            <Tooltip label="Search titles, plugins, requesters, projects, workflows and refs" hoverOnly>
              <label className="search-field">
                <Search size={14} aria-hidden="true" />
                <input
                  ref={search}
                  type="search"
                  autoComplete="off"
                  autoCorrect="off"
                  autoCapitalize="off"
                  spellCheck={false}
                  placeholder="Search history…"
                  aria-label="Search history"
                  value={q}
                  onChange={(e) => {
                    setQ(e.target.value);
                    setFilter("q", e.target.value);
                  }}
                />
              </label>
            </Tooltip>
            <Select
              label="Outcome"
              icon={<CircleDot size={14} />}
              value={status}
              onChange={(v) => setFilter("status", v)}
              options={[
                { value: "", label: "All outcomes", icon: <CircleDot size={14} /> },
                { value: "decided", label: "Decided" },
                { value: "withdrawn", label: "Withdrawn" },
                { value: "discarded", label: "Discarded" },
                { value: "expired", label: "Expired" },
              ]}
            />
            <Select
              label="Project"
              icon={<FolderGit2 size={14} />}
              value={repo}
              onChange={(v) => setFilter("repo", v)}
              options={[
                { value: "", label: "All projects", icon: <FolderGit2 size={14} /> },
                ...repos.map((r) => ({ value: r, label: r, icon: <FolderGit2 size={14} /> })),
                ...(data?.facets?.unassigned || repo === NO_PROJECT
                  ? [{ value: NO_PROJECT, label: "No project", icon: <FolderGit2 size={14} /> }]
                  : []),
              ]}
            />
            <Select
              label="Plugin"
              icon={<Blocks size={14} />}
              value={plugin}
              onChange={(v) => setFilter("plugin", v)}
              options={[
                { value: "", label: "All plugins", icon: <Blocks size={14} /> },
                ...plugins.map((p) => ({
                  value: p,
                  label: p,
                  icon: <PluginIcon icon={live.pluginIcon(p)} size={14} />,
                })),
              ]}
            />
            {filtered ? (
              <button
                type="button"
                className="chrome-button"
                onClick={() => {
                  setQ("");
                  updateParams(clearAll, { replace: true });
                }}
              >
                Clear
              </button>
            ) : null}
          </div>
        </header>
        {!data ? null : total === 0 ? (
          <EmptyState
            title={filtered ? "No matching decisions" : "Your decisions belong here"}
            icon={filtered ? <SearchX size={28} strokeWidth={1.5} /> : <Archive size={28} strokeWidth={1.5} />}
          >
            {filtered
              ? "Nothing matches these filters."
              : "Decided, withdrawn, discarded and expired reviews appear here, with the view they were decided in."}
          </EmptyState>
        ) : (
          <div className="history-table-wrap">
            <table className="history-table">
              <thead>
                <tr>
                  <th>Review / requester</th>
                  <th>Outcome</th>
                  <th className="history-col-repo">Repository</th>
                  <th className="history-col-recorded">Recorded</th>
                </tr>
              </thead>
              <tbody>
                {reviews.map((r, i) => (
                  <tr
                    key={r.id}
                    className={`${i === focused ? "is-focused" : ""} ${r.id === openId ? "is-open" : ""}`}
                    aria-current={r.id === openId ? "true" : undefined}
                    data-history-row={i}
                    data-testid="history-row"
                    onMouseEnter={() => !keyboard.current && setFocused(i)}
                    onClick={(event) => {
                      // the title is a link of its own, which has already gone there
                      if (event.defaultPrevented || (event.target as Element).closest("a")) return;
                      open(r.id);
                    }}
                  >
                    <td>
                      <Link
                        to={hrefFor(r.id)}
                        onClick={(event) => openFromLink(event, r.id)}
                        state={panelMode ? undefined : { from: "history" }}
                        className="history-title"
                      >
                        {r.title}
                      </Link>
                      <small>
                        {r.requested_by ? (
                          <span className="requester">
                            <AgentIcon requestedBy={r.requested_by} size={12} />
                            {r.requested_by}
                          </span>
                        ) : null}
                        {r.requested_by ? " · " : ""}
                        <span className="history-plugin" title={r.plugin}>
                          <PluginIcon icon={live.pluginIcon(r.plugin)} size={12} />
                          {r.plugin}
                        </span>{" "}
                        <FilesCount total={r.attachments_total} />
                      </small>
                    </td>
                    <td>
                      <span className="history-outcome">
                        <OutcomeBadge review={r} />
                        <SummaryCounts summary={r.decision?.summary} />
                      </span>
                    </td>
                    <td className="history-col-repo">
                      {r.origin.repo}
                      {r.origin.ref ? <small className="mono">{r.origin.ref}</small> : null}
                    </td>
                    <td className="history-col-recorded">
                      <span title={stamp(settledAt(r))}>{stamp(settledAt(r))}</span>
                      {r.decision ? (
                        <small>
                          {r.decision.decided_by ? `Decided by ${r.decision.decided_by}` : "Decided"}
                          <SourceBadge review={r} made={r.decision} command="decide" />
                        </small>
                      ) : null}
                      {r.status === "withdrawn" ? (
                        <small>
                          <Reason
                            text={r.withdrawn_by ? `Withdrawn by ${r.withdrawn_by}` : "Withdrawn"}
                            reason={r.withdrawn_reason}
                          />
                          <SourceBadge
                            review={r}
                            made={{ via: r.withdrawn_via, session: r.withdrawn_session }}
                            command="withdraw"
                          />
                        </small>
                      ) : null}
                      {r.status === "discarded" ? (
                        <small>
                          <Reason
                            text={r.discarded_by ? `Discarded by ${r.discarded_by}` : "Discarded"}
                            reason={r.discarded_reason}
                          />
                          <SourceBadge
                            review={r}
                            made={{ via: r.discarded_via, session: r.discarded_session }}
                            command="discard"
                          />
                        </small>
                      ) : null}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {data ? (
          <Pager label="History" page={page} size={size} total={total} onPage={setPage} onSize={setSize} />
        ) : null}
      </div>
      {openId ? <SidePanel id={openId} split={split} linkTo={hrefFor} onClose={closePanel} /> : null}
    </div>
  );
}

/** Who withdrew or discarded a review, with the reason they gave in a tooltip. */
function Reason({ text, reason }: { text: string; reason: string | null }) {
  if (!reason) return <>{text}</>;
  return (
    <Tooltip label={reason}>
      <span className="history-reason" tabIndex={0} data-testid="history-reason">
        {text}
      </span>
    </Tooltip>
  );
}
