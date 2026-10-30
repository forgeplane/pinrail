// The inbox's header: one filter bar above its tabs, Pending, New rounds and
// History. The filters keep their values from tab to tab; Outcome applies to
// the History tab only, and the layout to the waiting tabs.

import { Blocks, CircleDot, FolderGit2, List, Search, Star } from "lucide-react";
import type { ReactNode, RefObject } from "react";
import { PluginIcon } from "../components/PluginIcon";
import { Select } from "../components/Select";
import { Tooltip } from "../components/Tooltip";
import { NO_PROJECT, clearAll, useUrlParams } from "../lib/url";
import { useLive } from "../state/live";

export type InboxTab = "pending" | "new" | "history";

/** The tab an address shows. */
export function tabOf(params: URLSearchParams): InboxTab {
  if (params.get("tab") === "history") return "history";
  return params.get("rounds") === "new" ? "new" : "pending";
}

/** The layout of the waiting tabs, and how to change it. */
export type LayoutChoice = { value: "projects" | "list"; set: (next: "projects" | "list") => void };

export function InboxHead({
  tab,
  search,
  q,
  onSearch,
  projects,
  unassigned,
  plugins,
  filtered,
  onClear,
  layout,
}: {
  tab: InboxTab;
  search: RefObject<HTMLInputElement | null>;
  /** the search box's text, which it keeps apart from the address */
  q: string;
  onSearch: (q: string) => void;
  projects: string[];
  /** whether some reviews have no project */
  unassigned: boolean;
  plugins: string[];
  filtered: boolean;
  onClear: () => void;
  layout?: LayoutChoice;
}) {
  const live = useLive();
  const [params, updateParams] = useUrlParams();
  const repo = params.get("repo") ?? "";
  const plugin = params.get("plugin") ?? "";
  const status = params.get("status") ?? "";
  const starred = params.get("starred") === "1";
  const history = tab === "history";
  const newRounds = live.pending.filter((r) => r.revises).length;

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
  /** Another tab keeps the shared filters and leaves the page, the panel and the outcome behind. */
  const showTab = (to: InboxTab) => {
    if (to === tab) return;
    updateParams((next) => {
      for (const key of ["tab", "rounds", "page", "open", "status"]) next.delete(key);
      if (to === "history") next.set("tab", "history");
      if (to === "new") next.set("rounds", "new");
    });
  };
  const tabButton = (to: InboxTab, label: string, count?: ReactNode) => (
    <button
      type="button"
      role="tab"
      aria-selected={tab === to}
      className={tab === to ? "active" : ""}
      onClick={() => showTab(to)}
    >
      {label} {count === undefined ? null : <span>{count}</span>}
    </button>
  );

  return (
    <>
      <header className="page-head">
        <div className="heading-inline">
          <h1>Inbox</h1>
          <span className="inbox-total" data-testid="inbox-total">
            {live.pendingCount}
          </span>
        </div>
        <div className="inbox-controls">
          <Tooltip
            label={history ? "Search titles, plugins, requesters, projects, workflows and refs" : "Search the inbox"}
            keys={["/"]}
            hoverOnly
          >
            <label className="search-field">
              <Search size={14} aria-hidden="true" />
              <input
                ref={search}
                id="inbox-search"
                type="search"
                autoComplete="off"
                autoCorrect="off"
                autoCapitalize="off"
                spellCheck={false}
                placeholder={history ? "Search history…" : "Search inbox…"}
                aria-label={history ? "Search history" : "Search inbox"}
                value={q}
                onChange={(e) => {
                  onSearch(e.target.value);
                  setParam("q", e.target.value);
                }}
              />
            </label>
          </Tooltip>
          {history ? (
            <Select
              label="Outcome"
              icon={<CircleDot size={14} />}
              value={status}
              onChange={(v) => setParam("status", v)}
              options={[
                { value: "", label: "All outcomes", icon: <CircleDot size={14} /> },
                { value: "decided", label: "Decided" },
                { value: "withdrawn", label: "Withdrawn" },
                { value: "discarded", label: "Discarded" },
                { value: "expired", label: "Expired" },
              ]}
            />
          ) : null}
          <Select
            id="inbox-repo"
            label="Project"
            icon={<FolderGit2 size={14} />}
            value={repo}
            onChange={(v) => setParam("repo", v)}
            options={[
              { value: "", label: "All projects", icon: <FolderGit2 size={14} /> },
              ...projects.map((r) => ({ value: r, label: r, icon: <FolderGit2 size={14} /> })),
              ...(unassigned || repo === NO_PROJECT
                ? [{ value: NO_PROJECT, label: "No project", icon: <FolderGit2 size={14} /> }]
                : []),
            ]}
          />
          <Select
            id="inbox-plugin"
            label="Plugin"
            icon={<Blocks size={14} />}
            value={plugin}
            onChange={(v) => setParam("plugin", v)}
            options={[
              { value: "", label: "All plugins", icon: <Blocks size={14} /> },
              ...plugins.map((p) => ({
                value: p,
                label: p,
                icon: <PluginIcon icon={live.pluginIcon(p)} size={14} />,
              })),
            ]}
          />
          <Tooltip label={starred ? "Show every review" : "Show only the starred reviews"} side="bottom">
            <button
              type="button"
              className={`chrome-button star-filter ${starred ? "is-starred" : ""}`}
              aria-label="Starred"
              aria-pressed={starred}
              onClick={() => setParam("starred", starred ? "" : "1")}
            >
              <Star size={14} fill={starred ? "currentColor" : "none"} />
            </button>
          </Tooltip>
          {filtered ? (
            <button
              type="button"
              className="chrome-button"
              onClick={() => {
                onClear();
                updateParams(
                  (next) => {
                    // the filters go, and the tab stays
                    const tabs = ["tab", "rounds"].map((key) => [key, next.get(key)] as const);
                    clearAll(next);
                    for (const [key, value] of tabs) if (value) next.set(key, value);
                  },
                  { replace: true },
                );
              }}
            >
              Clear
            </button>
          ) : null}
        </div>
      </header>
      <nav className="list-tabs" role="tablist" aria-label="Inbox views">
        {tabButton("pending", "Pending", live.pendingCount)}
        {tabButton("new", "New rounds", newRounds)}
        {tabButton("history", "History")}
        <span className="list-sort">Newest first</span>
        {layout ? (
          <span className="segmented inbox-layout" role="radiogroup" aria-label="Inbox layout">
            <Tooltip label="Grouped by project" side="top">
              <button
                type="button"
                role="radio"
                aria-checked={layout.value === "projects"}
                aria-label="Grouped by project"
                className={layout.value === "projects" ? "is-on" : ""}
                onClick={() => layout.set("projects")}
              >
                <FolderGit2 size={13} />
              </button>
            </Tooltip>
            <Tooltip label="One list, newest first" side="top">
              <button
                type="button"
                role="radio"
                aria-checked={layout.value === "list"}
                aria-label="One list"
                className={layout.value === "list" ? "is-on" : ""}
                onClick={() => layout.set("list")}
              >
                <List size={13} />
              </button>
            </Tooltip>
          </span>
        ) : null}
      </nav>
    </>
  );
}
