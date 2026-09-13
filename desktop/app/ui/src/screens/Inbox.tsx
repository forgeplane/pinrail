import { Blocks, CheckCheck, FolderGit2, Search, SearchX } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import type { Review } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { PluginIcon } from "../components/PluginIcon";
import { Select } from "../components/Select";
import { Tooltip } from "../components/Tooltip";
import { SummaryCounts } from "../components/Badges";
import { age } from "../lib/format";
import { useLive } from "../state/live";

function matches(review: Review, q: string) {
  if (!q) return true;
  const text = [review.title, review.plugin, review.requested_by, review.origin.repo, review.origin.workflow, review.origin.ref]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
  return text.includes(q.toLowerCase());
}

export function Inbox() {
  const live = useLive();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  // The search box keeps its own text: a router navigation per keystroke is
  // deferred, and a controlled input bound to the URL would snap back.
  const [q, setQ] = useState(params.get("q") ?? "");
  const repo = params.get("repo") ?? "";
  const plugin = params.get("plugin") ?? "";
  const rounds = params.get("rounds") ?? "";
  const [focused, setFocused] = useState(0);
  const search = useRef<HTMLInputElement>(null);

  const setParam = (key: string, value: string) => {
    const next = new URLSearchParams(params);
    if (value) next.set(key, value);
    else next.delete(key);
    setParams(next, { replace: true });
  };

  const reviews = useMemo(
    () =>
      live.pending.filter(
        (r) =>
          matches(r, q) &&
          (!repo || r.origin.repo === repo) &&
          (!plugin || r.plugin === plugin) &&
          (rounds !== "new" || !!r.revises),
      ),
    [live.pending, q, repo, plugin, rounds],
  );
  const newRounds = live.pending.filter((r) => r.revises).length;
  const plugins = [...new Set(live.pending.map((r) => r.plugin))].sort();

  const groups = useMemo(() => {
    const byRepo = new Map<string, Review[]>();
    for (const r of reviews) {
      const key = r.origin.repo ?? "";
      byRepo.set(key, [...(byRepo.get(key) ?? []), r]);
    }
    return [...byRepo.entries()].sort(([a], [b]) => a.localeCompare(b));
  }, [reviews]);

  useEffect(() => {
    setFocused((f) => Math.min(f, Math.max(0, reviews.length - 1)));
  }, [reviews.length]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const el = event.target as HTMLElement | null;
      const typing = !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT");
      if (event.key === "/" && !typing) {
        event.preventDefault();
        search.current?.focus();
        return;
      }
      if (typing) {
        if (event.key === "Escape") (el as HTMLElement).blur();
        return;
      }
      if (event.key === "j") setFocused((f) => Math.min(f + 1, reviews.length - 1));
      if (event.key === "k") setFocused((f) => Math.max(f - 1, 0));
      if (event.key === "Enter" && reviews[focused]) navigate(`/reviews/${reviews[focused].id}`);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [reviews, focused, navigate]);

  const filtered = !!(q || repo || plugin || rounds);
  const now = Date.now();
  let index = -1;

  return (
    <div className="inbox">
      <header className="page-head">
        <div className="heading-inline">
          <h1>Inbox</h1>
          <span className="inbox-total">{live.pendingCount}</span>
        </div>
        <div className="inbox-controls">
          <Tooltip label="Search the inbox" keys={["/"]} hoverOnly>
            <label className="search-field">
              <Search size={14} aria-hidden="true" />
              <input
                ref={search}
                id="inbox-search"
                type="search"
                placeholder="Search inbox…"
                aria-label="Search inbox"
                value={q}
                onChange={(e) => {
                  setQ(e.target.value);
                  setParam("q", e.target.value);
                }}
              />
            </label>
          </Tooltip>
          <Select
            id="inbox-repo"
            label="Repository"
            icon={<FolderGit2 size={14} />}
            value={repo}
            onChange={(v) => setParam("repo", v)}
            options={[{ value: "", label: "All repositories", icon: <FolderGit2 size={14} /> }, ...live.repositories.map((r) => ({ value: r, label: r, icon: <FolderGit2 size={14} /> }))]}
          />
          <Select
            id="inbox-plugin"
            label="Plugin"
            icon={<Blocks size={14} />}
            value={plugin}
            onChange={(v) => setParam("plugin", v)}
            options={[{ value: "", label: "All plugins", icon: <Blocks size={14} /> }, ...plugins.map((p) => ({ value: p, label: p, icon: <PluginIcon icon={live.pluginIcon(p)} size={14} /> }))]}
          />
        </div>
      </header>
      <nav className="list-tabs" aria-label="Inbox views">
        <button type="button" className={rounds !== "new" ? "active" : ""} onClick={() => setParam("rounds", "")}>
          Pending <span>{live.pendingCount}</span>
        </button>
        <button type="button" className={rounds === "new" ? "active" : ""} onClick={() => setParam("rounds", "new")}>
          New rounds <span>{newRounds}</span>
        </button>
        <span className="list-sort">Newest first</span>
      </nav>
      {groups.length === 0 ? (
        <EmptyState
          title={filtered ? "No matching reviews" : "All caught up"}
          icon={filtered ? <SearchX size={28} strokeWidth={1.5} /> : <CheckCheck size={28} strokeWidth={1.5} />}
          actions={
            filtered ? (
              <button
                type="button"
                className="chrome-button"
                onClick={() => {
                  setQ("");
                  setParams({}, { replace: true });
                }}
              >
                Clear filters
              </button>
            ) : (
              <Link to="/history" className="chrome-button">
                View history
              </Link>
            )
          }
        >
          {filtered ? "Try a different search or clear the filters." : "New reviews appear here when an agent needs you."}
        </EmptyState>
      ) : (
        <div className="inbox-groups">
          {groups.map(([repoName, items]) => (
            <details key={repoName || "none"} className="inbox-repo" open>
              <summary>
                <strong>{repoName || "No repository"}</strong>
                <span>{items.length}</span>
              </summary>
              {items.map((review) => {
                index += 1;
                const here = index;
                return (
                  <Link
                    key={review.id}
                    to={`/reviews/${review.id}`}
                    className={`review-row ${here === focused ? "is-focused" : ""}`}
                    onMouseEnter={() => setFocused(here)}
                    data-review-row
                  >
                    <span className="review-row-marker" aria-label="Pending" />
                    <span className="review-row-main">
                      <span className="review-row-title">
                        {review.title}
                        {review.origin.ref ? <span className="review-row-ref">{review.origin.ref}</span> : null}
                      </span>
                      <span className="review-row-meta">
                        {review.requested_by ? <span>{review.requested_by}</span> : null}
                        {review.requested_by && review.origin.workflow ? <span>·</span> : null}
                        {review.origin.workflow ? <span>{review.origin.workflow}</span> : null}
                        {review.revises ? <span>· New round</span> : null}
                      </span>
                    </span>
                    <span className="review-row-plugin">
                      <PluginIcon icon={live.pluginIcon(review.plugin)} size={13} />
                      {review.plugin}
                    </span>
                    <span className="review-row-summary">
                      <SummaryCounts summary={review.summary} />
                    </span>
                    <time className="review-row-time" dateTime={review.created_at}>
                      {age(review.created_at, now)}
                    </time>
                  </Link>
                );
              })}
            </details>
          ))}
        </div>
      )}
      <footer className="inbox-footer">
        <kbd>J</kbd> <kbd>K</kbd> move · <kbd>↵</kbd> open · <kbd>/</kbd> search
      </footer>
    </div>
  );
}
