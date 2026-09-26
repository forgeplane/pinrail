import { Ban, Blocks, CheckCheck, FolderGit2, Search, SearchX } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate } from "react-router";
import type { Review } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { Pager, pageOf, pageSizeOf } from "../components/Pager";
import { DiscardDialog } from "../components/DiscardDialog";
import { PluginIcon } from "../components/PluginIcon";
import { Select } from "../components/Select";
import { Tooltip } from "../components/Tooltip";
import { FilesCount, SummaryCounts } from "../components/Badges";
import { age } from "../lib/format";
import { clearAll, useUrlParams } from "../lib/url";
import { useLive } from "../state/live";
import { NO_PROJECT, inProject } from "../lib/shortcuts";

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
  const [params, updateParams] = useUrlParams();
  // The search box keeps its own text: a router navigation per keystroke is
  // deferred, and a controlled input bound to the URL would snap back.
  const [q, setQ] = useState(params.get("q") ?? "");
  const repo = params.get("repo") ?? "";
  const plugin = params.get("plugin") ?? "";
  const rounds = params.get("rounds") ?? "";
  const [focused, setFocused] = useState(0);
  const [discarding, setDiscarding] = useState<Review | null>(null);
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

  const page = pageOf(params.get("page"));
  const size = pageSizeOf(params.get("per"));

  /** Changing a filter starts again from the first page. */
  const setParam = (key: string, value: string) => {
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

  const reviews = useMemo(
    () =>
      live.pending.filter(
        (r) =>
          matches(r, q) &&
          inProject(r.origin.repo, repo) &&
          (!plugin || r.plugin === plugin) &&
          (rounds !== "new" || !!r.revises),
      ),
    [live.pending, q, repo, plugin, rounds],
  );
  const newRounds = live.pending.filter((r) => r.revises).length;
  const plugins = [...new Set(live.pending.map((r) => r.plugin))].sort();

  // In the order the rows are drawn: by project, the project with the
  // newest review first, and newest first within one. The reviews come
  // newest first, so a project's place is where its newest one falls.
  // J, K and Enter walk this order, so the row they open is the row lit.
  const ordered = useMemo(() => {
    const byRepo = new Map<string, Review[]>();
    for (const r of reviews) {
      const key = r.origin.repo ?? "";
      byRepo.set(key, [...(byRepo.get(key) ?? []), r]);
    }
    return [...byRepo.entries()].flatMap(([, items]) => items);
  }, [reviews]);

  // a page past the end, after reviews were decided: the last page there is
  const pages = Math.max(1, Math.ceil(ordered.length / size));
  const current = Math.min(page, pages);
  const shown = useMemo(() => ordered.slice((current - 1) * size, current * size), [ordered, current, size]);

  const groups = useMemo(() => {
    const byRepo = new Map<string, Review[]>();
    for (const r of shown) {
      const key = r.origin.repo ?? "";
      byRepo.set(key, [...(byRepo.get(key) ?? []), r]);
    }
    return [...byRepo.entries()];
  }, [shown]);

  useEffect(() => {
    setFocused((f) => Math.min(f, Math.max(0, shown.length - 1)));
  }, [shown.length]);

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
      if (discarding) return;
      if (event.key === "j" || event.key === "k") keyboard.current = true;
      if (event.key === "j") setFocused((f) => Math.min(f + 1, shown.length - 1));
      if (event.key === "k") setFocused((f) => Math.max(f - 1, 0));
      if (event.key === "Enter" && shown[focused]) navigate(`/reviews/${shown[focused].id}`);
      if (event.key === "d" && shown[focused]) {
        // the key must not land in the reason field that opens
        event.preventDefault();
        setDiscarding(shown[focused]);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [shown, focused, navigate, discarding]);

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
            label="Project"
            icon={<FolderGit2 size={14} />}
            value={repo}
            onChange={(v) => setParam("repo", v)}
            options={[
              { value: "", label: "All projects", icon: <FolderGit2 size={14} /> },
              ...live.projects.map((r) => ({ value: r, label: r, icon: <FolderGit2 size={14} /> })),
              ...(live.unassigned > 0 || repo === NO_PROJECT ? [{ value: NO_PROJECT, label: "No project", icon: <FolderGit2 size={14} /> }] : []),
            ]}
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
                  updateParams(clearAll, { replace: true });
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
                <strong>{repoName || "No project"}</strong>
                <span>{reviews.filter((r) => (r.origin.repo ?? "") === repoName).length}</span>
              </summary>
              {items.map((review) => {
                index += 1;
                const here = index;
                return (
                  <Link
                    key={review.id}
                    to={`/reviews/${review.id}`}
                    className={`review-row ${here === focused ? "is-focused" : ""}`}
                    onMouseEnter={() => !keyboard.current && setFocused(here)}
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
                        <FilesCount total={review.attachments_total} />
                      </span>
                    </span>
                    <span className="review-row-plugin">
                      <PluginIcon icon={live.pluginIcon(review.plugin)} size={13} />
                      {review.plugin}
                    </span>
                    <span className="review-row-summary">
                      <SummaryCounts summary={review.summary} />
                    </span>
                    <span className="review-row-end">
                      <time className="review-row-time" dateTime={review.created_at}>
                        {age(review.created_at, now)}
                      </time>
                      <Tooltip label="Discard: the agent is told to stop" keys={["D"]} side="top">
                        <button
                          type="button"
                          className="bar-button"
                          aria-label={`Discard ${review.title}`}
                          onClick={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            setDiscarding(review);
                          }}
                        >
                          <Ban size={14} />
                        </button>
                      </Tooltip>
                    </span>
                  </Link>
                );
              })}
            </details>
          ))}
        </div>
      )}
      <Pager label="Inbox" page={current} size={size} total={ordered.length} onPage={setPage} onSize={setSize} />
      {discarding ? <DiscardDialog review={discarding} onClose={() => setDiscarding(null)} onDone={() => setDiscarding(null)} /> : null}
      <footer className="inbox-footer">
        <kbd>J</kbd> <kbd>K</kbd> move · <kbd>↵</kbd> open · <kbd>/</kbd> search
      </footer>
    </div>
  );
}
