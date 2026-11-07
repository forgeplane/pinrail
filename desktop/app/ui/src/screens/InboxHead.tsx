// The inbox's header: the search and the Filter and Display buttons above
// its tabs, Pending, New rounds and History, with a chip under the tabs for
// each filter that is on. The filters keep their values from tab to tab;
// Outcome applies to the History tab only, and the layout to the waiting
// tabs.

import { FolderGit2, Search } from "lucide-react";
import type { ReactNode, RefObject } from "react";
import { FilterChips, ListMenuButtons, groupingDisplay, type Filter, type Toggle } from "../components/ListMenus";
import { PluginIcon } from "../components/PluginIcon";
import { ProjectAvatar } from "../components/ProjectAvatar";
import { Tooltip } from "../components/Tooltip";
import { clearAll, scopeOf, scopeOptions, setScope, useUrlParams } from "../lib/url";
import { filtering, waitingFilter } from "../lib/waiting";
import { useLive } from "../state/live";

export type InboxTab = "pending" | "new" | "history";

/** The tab an address shows. */
export function tabOf(params: URLSearchParams): InboxTab {
  if (params.get("tab") === "history") return "history";
  return params.get("rounds") === "new" ? "new" : "pending";
}

/** How the waiting tabs are laid out: grouped by project or by repository, or one list. */
export type { Layout } from "../lib/grouping";
import type { Layout } from "../lib/grouping";
/** The layout of the waiting tabs, and how to change it. */
export type LayoutChoice = { value: Layout; set: (next: Layout) => void };

export function InboxHead({
  tab,
  search,
  q,
  onSearch,
  projects,
  unassigned,
  plugins,
  onClear,
  layout,
}: {
  tab: InboxTab;
  search: RefObject<HTMLInputElement | null>;
  /** the search box's text, which it keeps apart from the address */
  q: string;
  onSearch: (q: string) => void;
  /** the repositories among the reviews */
  projects: string[];
  /** whether some reviews have no repository */
  unassigned: boolean;
  plugins: string[];
  onClear: () => void;
  layout?: LayoutChoice;
}) {
  const live = useLive();
  const [params, updateParams] = useUrlParams();
  const scope = scopeOf(params);
  const plugin = params.get("plugin") ?? "";
  const status = params.get("status") ?? "";
  const starred = params.get("starred") === "1";
  const history = tab === "history";
  // what the filters keep counts in the tabs; with none set, every review
  // that waits counts, those beyond the loaded ones too
  const kept = live.pending.filter(waitingFilter(params, q));
  const pending = filtering(params, q) ? kept.length : live.pendingCount;
  const newRounds = kept.filter((r) => r.revises).length;

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

  // the filters, in the Filter menu and as chips while they are on; a
  // search clears with them, and the tab stays
  const clear = () => {
    onClear();
    updateParams(
      (next) => {
        const tabs = ["tab", "rounds"].map((key) => [key, next.get(key)] as const);
        clearAll(next);
        for (const [key, value] of tabs) if (value) next.set(key, value);
      },
      { replace: true },
    );
  };
  const filters: Filter[] = [
    ...(history
      ? [
          {
            key: "status",
            label: "Outcome",
            value: status,
            off: "",
            choices: [
              { value: "", label: "All outcomes" },
              { value: "decided", label: "Decided" },
              { value: "withdrawn", label: "Withdrawn" },
              { value: "discarded", label: "Discarded" },
              { value: "expired", label: "Expired" },
            ],
            onChange: (v: string) => setParam("status", v),
          },
        ]
      : []),
    {
      key: "scope",
      label: "Project or repository",
      value: scope,
      off: "",
      searchable: true,
      choices: scopeOptions(live.namedProjects, projects, unassigned, scope).map((o) => ({
        value: o.value,
        label: o.label,
        mark: o.project ? <ProjectAvatar project={o.project} size={14} /> : <FolderGit2 size={13} />,
      })),
      onChange: (v) =>
        updateParams(
          (next) => {
            setScope(next, v);
            next.delete("page");
          },
          { replace: true },
        ),
    },
    {
      key: "plugin",
      label: "Plugin",
      value: plugin,
      off: "",
      searchable: plugins.length > 8,
      choices: [
        { value: "", label: "All plugins" },
        ...plugins.map((p) => ({ value: p, label: p, mark: <PluginIcon icon={live.pluginIcon(p)} size={13} /> })),
      ],
      onChange: (v) => setParam("plugin", v),
    },
  ];
  const toggles: Toggle[] = [
    { key: "starred", label: "Starred", on: starred, onChange: (on) => setParam("starred", on ? "1" : "") },
  ];

  return (
    <>
      <header className="page-head">
        <div className="heading-inline">
          <h1>Inbox</h1>
          {/* filtered, what the filters keep of everything that waits */}
          <span className="inbox-total" data-testid="inbox-total">
            {filtering(params, q) ? `${pending} of ${live.pendingCount}` : live.pendingCount}
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
          <ListMenuButtons
            label="reviews"
            filters={filters}
            toggles={toggles}
            onReset={clear}
            displays={layout && !history ? [groupingDisplay(layout.value, layout.set)] : []}
          />
        </div>
      </header>
      <nav className="list-tabs" role="tablist" aria-label="Inbox views">
        {tabButton("pending", "Pending", pending)}
        {tabButton("new", "New rounds", newRounds)}
        {tabButton("history", "History")}
        <span className="list-sort">Newest first</span>
      </nav>
      <FilterChips filters={filters} toggles={toggles} onReset={clear} />
    </>
  );
}
