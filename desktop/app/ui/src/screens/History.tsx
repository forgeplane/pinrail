import { Archive, Blocks, CircleDot, FolderGit2, Search, SearchX } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { api } from "../api/client";
import type { Review } from "../api/types";
import { OutcomeBadge } from "../components/Badges";
import { EmptyState } from "../components/EmptyState";
import { PluginIcon } from "../components/PluginIcon";
import { Select } from "../components/Select";
import { Tooltip } from "../components/Tooltip";
import { stamp } from "../lib/format";
import { useLive } from "../state/live";

const settledAt = (r: Review) => r.decision?.decided_at ?? r.withdrawn_at ?? r.discarded_at ?? r.expires_at;

/** Any word of the query in any of the review's names. */
function matches(review: Review, q: string) {
  if (!q) return true;
  const text = [review.title, review.plugin, review.requested_by, review.origin.repo, review.origin.workflow, review.origin.ref, review.decision?.decided_by]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((word) => text.includes(word));
}

export function History() {
  const live = useLive();
  const [params, setParams] = useSearchParams();
  const navigate = useNavigate();
  const [all, setAll] = useState<Review[]>([]);
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
  const filtered = !!(q || status || plugin || repo);

  useEffect(() => {
    api
      .listReviews({ status: status || "decided,withdrawn,discarded,expired", include_revised: "true", limit: "500" })
      .then(setAll)
      .catch(() => setAll([]));
  }, [status, live.tick]);

  const setFilter = (key: string, value: string) => {
    const next = new URLSearchParams(params);
    if (value) next.set(key, value);
    else next.delete(key);
    setParams(next, { replace: true });
  };

  const reviews = useMemo(
    () => all.filter((r) => matches(r, q) && (!plugin || r.plugin === plugin) && (!repo || r.origin.repo === repo)),
    [all, q, plugin, repo],
  );
  const plugins = useMemo(() => [...new Set(all.map((r) => r.plugin))].sort(), [all]);

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
  const repos = useMemo(() => [...new Set(all.map((r) => r.origin.repo).filter((r): r is string => !!r))].sort(), [all]);

  return (
    <div className="history">
      <header className="page-head">
        <h1>History</h1>
        <div className="inbox-controls">
          <Tooltip label="Search titles, plugins, requesters, repositories, workflows and refs" hoverOnly>
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
            label="Repository"
            icon={<FolderGit2 size={14} />}
            value={repo}
            onChange={(v) => setFilter("repo", v)}
            options={[{ value: "", label: "All repositories", icon: <FolderGit2 size={14} /> }, ...repos.map((r) => ({ value: r, label: r, icon: <FolderGit2 size={14} /> }))]}
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
                setParams({}, { replace: true });
              }}
            >
              Clear
            </button>
          ) : null}
        </div>
      </header>
      {reviews.length === 0 ? (
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
                      {r.plugin}
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
    </div>
  );
}
