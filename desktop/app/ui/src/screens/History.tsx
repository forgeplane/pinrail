import { Archive, Blocks, CircleDot, FolderGit2, Search, SearchX } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Link, useNavigate } from "react-router";
import { api } from "../api/client";
import type { Review, ReviewListing } from "../api/types";
import { FilesCount, OutcomeBadge } from "../components/Badges";
import { EmptyState } from "../components/EmptyState";
import { Pager, pageOf, pageSizeOf } from "../components/Pager";
import { PluginIcon } from "../components/PluginIcon";
import { Select } from "../components/Select";
import { Tooltip } from "../components/Tooltip";
import { stamp } from "../lib/format";
import { clearAll, useUrlParams } from "../lib/url";
import { useLive } from "../state/live";
import { NO_PROJECT } from "../lib/shortcuts";

const settledAt = (r: Review) => r.decision?.decided_at ?? r.withdrawn_at ?? r.discarded_at ?? r.expires_at;

const ENDED = "decided,withdrawn,discarded,expired";
/** How long the search waits for typing to pause before it asks the server. */
const SEARCH_DELAY_MS = 250;

export function History() {
  const live = useLive();
  const [params, updateParams] = useUrlParams();
  const navigate = useNavigate();
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
      .listReviews({ status: status || ENDED, include_revised: "true", q: debounced, plugin, repo, offset: String((page - 1) * size), limit: String(size), include: "facets" })
      .then((p) => !cancelled && setData(p))
      .catch(() => !cancelled && setData(null));
    return () => {
      cancelled = true;
    };
  }, [status, debounced, plugin, repo, page, size, live.tick]);

  const reviews = data?.reviews ?? [];
  const total = data?.total ?? 0;
  const pages = Math.max(1, Math.ceil(total / size));

  // a page past the end, after a filter or a sweep shrank the list: the last page
  useEffect(() => {
    if (data && total > 0 && reviews.length === 0 && page > 1) setPage(Math.ceil(total / size));
  }, [data]); // eslint-disable-line react-hooks/exhaustive-deps

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

  useEffect(() => {
    setFocused((f) => Math.min(f, Math.max(0, reviews.length - 1)));
  }, [reviews.length]);

  // the same keys as the inbox: j/k move, enter opens, / searches
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey) return;
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
      if (event.key === "Enter" && reviews[focused]) navigate(`/reviews/${reviews[focused].id}`, { state: { from: "history" } });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [reviews, focused, navigate]);

  useEffect(() => {
    document.querySelector<HTMLElement>(`[data-history-row="${focused}"]`)?.scrollIntoView({ block: "nearest" });
  }, [focused]);
  const repos = data?.facets?.repos ?? [];

  return (
    <div className="history">
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
              ...(data?.facets?.unassigned || repo === NO_PROJECT ? [{ value: NO_PROJECT, label: "No project", icon: <FolderGit2 size={14} /> }] : []),
            ]}
          />
          <Select
            label="Plugin"
            icon={<Blocks size={14} />}
            value={plugin}
            onChange={(v) => setFilter("plugin", v)}
            options={[{ value: "", label: "All plugins", icon: <Blocks size={14} /> }, ...plugins.map((p) => ({ value: p, label: p, icon: <PluginIcon icon={live.pluginIcon(p)} size={14} /> }))]}
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
          {filtered ? "Nothing matches these filters." : "Decided, withdrawn, discarded and expired reviews appear here, with the view they were decided in."}
        </EmptyState>
      ) : (
        <div className="history-table-wrap">
          <table className="history-table">
            <thead>
              <tr>
                <th>Review / requester</th>
                <th>Outcome</th>
                <th>Repository</th>
                <th>Recorded</th>
              </tr>
            </thead>
            <tbody>
              {reviews.map((r, i) => (
                <tr key={r.id} className={i === focused ? "is-focused" : ""} data-history-row={i} onMouseEnter={() => !keyboard.current && setFocused(i)} onClick={() => navigate(`/reviews/${r.id}`, { state: { from: "history" } })}>
                  <td>
                    <Link to={`/reviews/${r.id}`} state={{ from: "history" }} className="history-title">
                      {r.title}
                    </Link>
                    <small>
                      {r.requested_by}
                      {r.requested_by ? " · " : ""}
                      <span className="history-plugin">
                        <PluginIcon icon={live.pluginIcon(r.plugin)} size={12} />
                        {r.plugin}
                      </span>{" "}
                      <FilesCount total={r.attachments_total} />
                    </small>
                  </td>
                  <td>
                    <OutcomeBadge review={r} />
                  </td>
                  <td>
                    {r.origin.repo}
                    {r.origin.ref ? <small className="mono">{r.origin.ref}</small> : null}
                  </td>
                  <td title={stamp(settledAt(r))}>
                    {stamp(settledAt(r))}
                    {r.decision ? <small>Decided by {r.decision.decided_by}</small> : null}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {data ? <Pager label="History" page={page} size={size} total={total} onPage={setPage} onSize={setSize} /> : null}
    </div>
  );
}
