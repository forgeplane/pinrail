// The inbox's header: the search, Mark all as read and the Filter button
// above its tabs, All, Waiting on you, Unread and History, with a chip under
// the tabs for each filter that is on. The notifications' tabs share the
// Kind, Project or repository and Plugin filters, each with several values;
// the History tab has its own, with Outcome.

import {
  CheckCheck,
  CircleCheck,
  CircleStop,
  CircleX,
  Clock,
  Flag,
  Hand,
  FolderGit2,
  GitPullRequest,
  Layers,
  ListChecks,
  ListPlus,
  type LucideIcon,
  MessageCircleQuestion,
  MessageSquare,
  MessageSquareMore,
  Repeat,
  Search,
  ShieldQuestion,
  StickyNote,
  Undo2,
} from "lucide-react";
import type { ReactNode, RefObject } from "react";
import { FilterChips, ListMenuButtons, type Filter, type Toggle } from "../components/ListMenus";
import { PluginIcon } from "../components/PluginIcon";
import { ProjectAvatar } from "../components/ProjectAvatar";
import { SearchInput } from "../components/SearchInput";
import { Tooltip } from "../components/Tooltip";
import { SHIFT } from "../lib/keys";
import { clearAll, scopeOf, scopeOptions, setScope, useUrlParams } from "../lib/url";
import { useLive } from "../state/live";

export type InboxTab = "all" | "waiting" | "unread" | "history";

/** The tab an address shows. */
export function tabOf(params: URLSearchParams): InboxTab {
  const tab = params.get("tab");
  return tab === "history" || tab === "waiting" || tab === "unread" ? tab : "all";
}

/** The notifications' filters as the address keeps them: each a
 *  comma-separated list. */
export const NOTIFICATION_FILTERS = ["kind", "scope", "plugin", "starred", "done"] as const;
/** The History tab's filters. */
const HISTORY_FILTERS = ["repo", "project", "plugin", "status", "starred"] as const;

/** The kinds the Kind filter offers, each with its icon. */
export const KINDS: { value: string; label: string; Icon: LucideIcon }[] = [
  { value: "review_requested", label: "Reviews", Icon: GitPullRequest },
  { value: "new_round", label: "New rounds", Icon: Repeat },
  { value: "question_asked", label: "Questions", Icon: MessageCircleQuestion },
  { value: "permission_requested", label: "Permissions", Icon: ShieldQuestion },
  { value: "input_requested", label: "Agents waiting for an answer", Icon: MessageSquareMore },
  { value: "item_added", label: "New work items", Icon: ListPlus },
  { value: "item_commented", label: "Comments", Icon: MessageSquare },
  { value: "item_closed", label: "Closed work items", Icon: CircleCheck },
  { value: "item_claimed", label: "Claimed work items", Icon: Hand },
  { value: "item_changed", label: "Other work item changes", Icon: ListChecks },
  { value: "agent_finished", label: "Finished agents", Icon: Flag },
  { value: "agent_failed", label: "Failed turns", Icon: CircleX },
  { value: "agent_stopped", label: "Stopped agents", Icon: CircleStop },
  { value: "agent_note", label: "Notes", Icon: StickyNote },
  { value: "review_withdrawn", label: "Withdrawn reviews", Icon: Undo2 },
  { value: "review_expired", label: "Expired reviews", Icon: Clock },
];

export function InboxHead({
  tab,
  search,
  q,
  onSearch,
  projects,
  unassigned,
  plugins,
  onClear,
  counts,
  onReadAll,
}: {
  tab: InboxTab;
  search: RefObject<HTMLInputElement | null>;
  /** the search box's text, which it keeps apart from the address */
  q: string;
  onSearch: (q: string) => void;
  /** the repositories the filter offers */
  projects: string[];
  /** whether some have no repository */
  unassigned: boolean;
  plugins: string[];
  onClear: () => void;
  /** how many notifications wait, and how many are unread, for the tabs */
  counts?: { waiting: number; unread: number };
  /** marks read what the list shows, when some of it is unread */
  onReadAll?: () => void;
}) {
  const live = useLive();
  const [params, updateParams] = useUrlParams();
  const history = tab === "history";
  const starred = params.get("starred") === "1";

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
  /** Another tab leaves the page, the panel and the outcome behind; between
   *  the notifications' tabs and History, whose filters differ, the filters
   *  too. */
  const showTab = (to: InboxTab) => {
    if (to === tab) return;
    updateParams((next) => {
      for (const key of ["tab", "rounds", "page", "open", "status"]) next.delete(key);
      if ((to === "history") !== history) {
        for (const key of [...NOTIFICATION_FILTERS, ...HISTORY_FILTERS]) next.delete(key);
      }
      if (to !== "all") next.set("tab", to);
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
        const kept = next.get("tab");
        clearAll(next);
        if (kept) next.set("tab", kept);
      },
      { replace: true },
    );
  };
  const scopeChoices = (value: string) =>
    scopeOptions(live.namedProjects, projects, unassigned, value).map((o) => ({
      value: o.value,
      label: o.label,
      mark: o.project ? (
        <ProjectAvatar project={o.project} size={14} />
      ) : o.kind === "all" ? (
        <Layers size={13} />
      ) : (
        <FolderGit2 size={13} />
      ),
    }));
  const pluginChoices = [
    { value: "", label: "All plugins", mark: <Layers size={13} /> },
    ...plugins.map((p) => ({ value: p, label: p, mark: <PluginIcon icon={live.pluginIcon(p)} size={13} /> })),
  ];
  const filters: Filter[] = history
    ? [
        {
          key: "status",
          label: "Outcome",
          value: params.get("status") ?? "",
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
        {
          key: "scope",
          label: "Project or repository",
          value: scopeOf(params),
          off: "",
          searchable: true,
          choices: scopeChoices(scopeOf(params)),
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
          value: params.get("plugin") ?? "",
          off: "",
          searchable: plugins.length > 8,
          choices: pluginChoices,
          onChange: (v) => setParam("plugin", v),
        },
      ]
    : [
        {
          key: "kind",
          label: "Kind",
          value: params.get("kind") ?? "",
          off: "",
          multi: true,
          searchable: true,
          choices: [
            { value: "", label: "All kinds", mark: <Layers size={13} /> },
            ...KINDS.map(({ value, label, Icon }) => ({ value, label, mark: <Icon size={13} /> })),
          ],
          onChange: (v) => setParam("kind", v),
        },
        {
          key: "scope",
          label: "Project or repository",
          value: params.get("scope") ?? "",
          off: "",
          multi: true,
          searchable: true,
          choices: scopeChoices(""),
          onChange: (v) => setParam("scope", v),
        },
        {
          key: "plugin",
          label: "Plugin",
          value: params.get("plugin") ?? "",
          off: "",
          multi: true,
          searchable: plugins.length > 8,
          choices: pluginChoices,
          onChange: (v) => setParam("plugin", v),
        },
      ];
  const toggles: Toggle[] = [
    { key: "starred", label: "Starred", on: starred, onChange: (on) => setParam("starred", on ? "1" : "") },
    ...(history
      ? []
      : [
          {
            key: "done",
            label: "Show done",
            on: params.get("done") === "1",
            onChange: (on: boolean) => setParam("done", on ? "1" : ""),
          },
        ]),
  ];

  return (
    <>
      <header className="page-head">
        <div className="heading-inline">
          <h1>Inbox</h1>
          <span className="inbox-total" data-testid="inbox-total">
            {counts?.waiting ?? live.pendingCount}
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
              <SearchInput
                ref={search}
                id="inbox-search"
                placeholder={history ? "Search history…" : "Search inbox…"}
                label={history ? "Search history" : "Search inbox"}
                value={q}
                onChange={(text) => {
                  onSearch(text);
                  setParam("q", text);
                }}
              />
            </label>
          </Tooltip>
          {onReadAll && !history ? (
            <Tooltip label="Mark what the list shows read" keys={[SHIFT, "U"]} side="bottom">
              <button type="button" className="chrome-button" onClick={onReadAll}>
                <CheckCheck size={14} aria-hidden="true" /> Mark all as read
              </button>
            </Tooltip>
          ) : null}
          <ListMenuButtons
            label={history ? "reviews" : "notifications"}
            filters={filters}
            toggles={toggles}
            onReset={clear}
          />
        </div>
      </header>
      <nav className="list-tabs" role="tablist" aria-label="Inbox views">
        {tabButton("all", "All")}
        {tabButton("waiting", "Waiting on you", counts?.waiting)}
        {tabButton("unread", "Unread", counts?.unread)}
        {tabButton("history", "History")}
        <span className="list-sort">Newest first</span>
      </nav>
      <FilterChips filters={filters} toggles={toggles} onReset={clear} />
    </>
  );
}
