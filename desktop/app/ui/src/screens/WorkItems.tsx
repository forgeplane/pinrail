// Work items: what the person and agents record and work through, as one
// list grouped like the inbox, with sub-items nested under their parent or
// each as a row of its own, a quick add at the top, and filters.

import {
  ChevronDown,
  ChevronRight,
  Clock,
  FolderGit2,
  ListChecks,
  Lock,
  Plus,
  Search,
  SearchX,
  User,
  X,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { ApiError, api } from "../api/client";
import type { WorkItem } from "../api/types";
import { AgentIcon } from "../components/AgentIcon";
import { EmptyState } from "../components/EmptyState";
import { GroupHeading } from "../components/GroupHeading";
import { ItemStatusIcon } from "../components/ItemStatusIcon";
import { FilterChips, ListMenuButtons, groupingDisplay, type Filter, type Toggle } from "../components/ListMenus";
import { ORDERINGS, orderItems, type Ordering } from "../lib/itemOrder";
import { List as ListIcon, ListTree } from "lucide-react";
import { ProjectAvatar } from "../components/ProjectAvatar";
import { Select } from "../components/Select";
import { Tooltip } from "../components/Tooltip";
import { age, stamp } from "../lib/format";
import { groupKey, grouped, mixedGroups } from "../lib/grouping";
import { MOD, SHIFT, modalOpen } from "../lib/keys";
import { KeyHints } from "../components/KeyHints";
import { ItemPanel } from "../components/ItemPanel";
import { CancelItemDialog, openSubItems } from "../components/CancelItemDialog";
import { SidePanel } from "../components/SidePanel";
import { useOptionsMenu } from "../components/OptionsMenu";
import { inScope, scopeOf, scopeOptions, setScope, useUrlParams } from "../lib/url";
import { useLive } from "../state/live";
import { useSettings } from "../state/settings";
import { useToast } from "../state/toasts";

/** How long the search waits for typing to pause before it asks the server. */
const SEARCH_DELAY_MS = 250;

/** The choices of the status filter, and what each asks the core for. */
const STATUSES: { value: string; label: string; query?: string }[] = [
  { value: "", label: "Open" },
  { value: "ready", label: "Ready", query: "ready" },
  { value: "blocked", label: "Blocked", query: "blocked" },
  { value: "in_progress", label: "In progress", query: "in_progress" },
  { value: "open", label: "Not started", query: "open" },
  { value: "done", label: "Done", query: "done" },
  { value: "cancelled", label: "Cancelled", query: "cancelled" },
  { value: "all", label: "All", query: "open,in_progress,done,cancelled" },
];

/** The icon of a choice of the status filter: the status's own, or what
 *  marks a blocked item. */
function statusIcon(value: string) {
  if (value === "blocked") return <Lock size={13} className="item-status-blocked" aria-hidden="true" />;
  if (value === "ready" || value === "open") return <ItemStatusIcon status="open" decorative />;
  if (value === "in_progress" || value === "done" || value === "cancelled")
    return <ItemStatusIcon status={value} decorative />;
  return <ListChecks size={13} aria-hidden="true" />;
}

/** What a dragged row carries: its item's key. */
const DRAGGED = "application/x-pinrail-item";

/** An item in the order J and K walk, at its depth in the tree. */
type Shown = { item: WorkItem; depth: number; folded: boolean; children: number; parentTitle?: string };

const reason = (error: unknown) => (error instanceof ApiError ? error.message : "Pinrail could not be reached");

export function WorkItems() {
  const live = useLive();
  const toast = useToast();
  const [params, updateParams, currentParams] = useUrlParams();
  const { settings: prefs, update } = useSettings();
  const layout = prefs.items.layout;
  const nested = prefs.items.nesting === "nested";
  const [q, setQ] = useState(params.get("q") ?? "");
  const query = params.get("q") ?? "";
  const status = params.get("status") ?? "";
  const scope = scopeOf(params);
  const by = params.get("by") ?? "";
  const waiting = params.get("waiting") === "1";
  const openKey = params.get("open");
  const filtered = !!(query || status || scope || by || waiting);
  const [items, setItems] = useState<WorkItem[] | null>(null);
  const [repos, setRepos] = useState<string[]>([]);
  // each project's repositories, by its id: where an item dropped on it goes
  const [projectRepos, setProjectRepos] = useState<Record<string, string[]>>({});
  const [focused, setFocused] = useState(0);
  const [folded, setFolded] = useState<Set<string>>(new Set());
  // the page reloads after its own changes, whether or not an event brings them
  const [reloads, setReloads] = useState(0);
  const reload = useCallback(() => setReloads((n) => n + 1), []);

  const [debounced, setDebounced] = useState(query);
  useEffect(() => {
    const t = window.setTimeout(() => setDebounced(query), SEARCH_DELAY_MS);
    return () => window.clearTimeout(t);
  }, [query]);
  useEffect(() => {
    let cancelled = false;
    const wanted = STATUSES.find((s) => s.value === status)?.query;
    api
      .items({ status: wanted, q: debounced || undefined })
      .then((d) => !cancelled && setItems(d.items))
      .catch(() => !cancelled && setItems(null));
    return () => {
      cancelled = true;
    };
  }, [status, debounced, live.lastNotice, reloads]);
  // the repositories an item can be added to: every one the app knows
  useEffect(() => {
    let cancelled = false;
    api
      .projects()
      .then((l) => {
        if (cancelled) return;
        setRepos([
          ...new Set([...l.projects.flatMap((p) => p.repos.map((r) => r.repo)), ...l.loose.map((r) => r.repo)]),
        ]);
        setProjectRepos(Object.fromEntries(l.projects.map((p) => [p.id, p.repos.map((r) => r.repo)])));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [live.projectsVersion]);

  const setParam = (key: string, value: string | null) =>
    updateParams(
      (next) => {
        if (value) next.set(key, value);
        else next.delete(key);
      },
      { replace: true },
    );

  // the filters the list itself applies: the scope, who added it, what waits
  const kept = useMemo(
    () =>
      (items ?? []).filter(
        (i) =>
          (!scope || inScope(i.repo, i.project, scope)) && (!by || i.created_by === by) && (!waiting || i.waiting > 0),
      ),
    [items, scope, by, waiting],
  );
  // in the order chosen; the groups and the sub-items keep it
  const ordered = useMemo(() => orderItems(kept, prefs.items.ordering), [kept, prefs.items.ordering]);
  const groups = useMemo(
    () =>
      layout === "list"
        ? [["", ordered] as [string, WorkItem[]]]
        : grouped(ordered, (i) => groupKey(layout, i.repo, i.project)),
    [ordered, layout],
  );
  const mixed = useMemo(() => mixedGroups(groups, (i) => i.repo), [groups]);

  /** A group's rows in order: nested under their parents, or flat. */
  const rowsOf = useCallback(
    (list: WorkItem[]): Shown[] => {
      if (!nested) return list.map((item) => ({ item, depth: 0, folded: false, children: 0 }));
      const here = new Map(list.map((i) => [i.key, i]));
      const children = new Map<string, WorkItem[]>();
      const roots: WorkItem[] = [];
      for (const item of list) {
        if (item.parent && here.has(item.parent))
          children.set(item.parent, [...(children.get(item.parent) ?? []), item]);
        else roots.push(item);
      }
      const out: Shown[] = [];
      const walk = (item: WorkItem, depth: number) => {
        const kids = children.get(item.key) ?? [];
        const isFolded = folded.has(item.key);
        out.push({ item, depth, folded: isFolded, children: kids.length });
        if (!isFolded) for (const kid of kids) walk(kid, depth + 1);
      };
      for (const root of roots) walk(root, 0);
      return out;
    },
    [nested, folded],
  );
  const shownGroups = useMemo(
    () => groups.map(([key, list]) => [key, rowsOf(list)] as [string, Shown[]]),
    [groups, rowsOf],
  );
  const rows = useMemo(() => shownGroups.flatMap(([, r]) => r), [shownGroups]);
  const agents = useMemo(
    () => [...new Set((items ?? []).map((i) => i.created_by).filter((b) => b !== "person"))].sort(),
    [items],
  );

  /** Opens an item in the side panel, by its key. */
  const openKeyed = useCallback(
    (key: string) =>
      updateParams(
        (next) => {
          next.set("open", key);
        },
        { replace: !!currentParams().get("open") },
      ),
    [updateParams, currentParams],
  );
  const open = useCallback((item: WorkItem) => openKeyed(item.key), [openKeyed]);
  const closePanel = useCallback(
    () =>
      updateParams((next) => {
        next.delete("open");
      }),
    [updateParams],
  );
  // the element that holds the list and the side panel
  const split = useRef<HTMLDivElement>(null);
  const toggleFold = (key: string) =>
    setFolded((f) => {
      const next = new Set(f);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  /** A change the person makes to an item from the list. */
  const change = useCallback(
    async (item: WorkItem, body: Record<string, unknown>) => {
      try {
        await api.editItem(item.key, body);
      } catch (error) {
        toast(reason(error), "danger");
      }
      reload();
    },
    [toast, reload],
  );

  // an item with open sub-items waiting for the person to confirm its cancel
  const [cancelling, setCancelling] = useState<WorkItem | null>(null);
  // an item dragged onto another becomes its sub-item, and a sub-item
  // dragged to the top of the list leaves its parent
  const [dropOn, setDropOn] = useState<string | null>(null);
  const [dragging, setDragging] = useState<WorkItem | null>(null);
  const dragTimer = useRef(0);
  const putUnder = useCallback(
    async (key: string, parent: string | null, from?: string | null) => {
      try {
        await api.editItem(key, { parent });
        toast(parent ? `${key} is now part of ${parent}` : `${key} is no longer part of ${from}`);
      } catch (error) {
        toast(reason(error), "danger");
      }
      reload();
    },
    [toast, reload],
  );

  /** Moves an item to another repository, where it takes the next number. */
  const moveTo = useCallback(
    async (key: string, repo: string) => {
      try {
        const { item } = await api.editItem(key, { repo });
        toast(`${key} moved to ${repo} as ${item.key}`);
        return true;
      } catch (error) {
        toast(reason(error), "danger");
        return false;
      } finally {
        reload();
      }
    },
    [toast, reload],
  );
  // the group the dragged row is in, which is no place to drop it
  const draggedGroup = useRef<string | null>(null);
  /** The repositories a group takes a dropped item to. */
  const reposOf = (group: string) =>
    group.startsWith("project:") ? (projectRepos[group.slice(8)] ?? []) : group ? [group] : [];
  // a project of several repositories asks which one, in a menu at its heading
  const [choosing, setChoosing] = useState<{ key: string; repos: string[]; anchor: HTMLElement } | null>(null);
  const dropOnGroup = (group: string, anchor: HTMLElement, key: string) => {
    const to = reposOf(group);
    if (to.length === 1) void moveTo(key, to[0]);
    else if (to.length > 1) setChoosing({ key, repos: to, anchor });
  };

  // the quick add's field, opened by C or a click
  const [adding, setAdding] = useState(false);

  // the focused row stays in view as J and K move it
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    list.current?.querySelector(`[data-row="${focused}"]`)?.scrollIntoView({ block: "nearest" });
  }, [focused]);
  useEffect(() => {
    if (focused >= rows.length && rows.length) setFocused(rows.length - 1);
  }, [rows.length, focused]);
  // the open item is focused, however it was opened, so J and K move on from
  // it; once per item, so a live update does not take the focus from a hovered row
  const focusedOpen = useRef<string | null>(null);
  useEffect(() => {
    if (!openKey) focusedOpen.current = null;
    if (!openKey || focusedOpen.current === openKey) return;
    const at = rows.findIndex((r) => r.item.key === openKey);
    if (at < 0) return;
    focusedOpen.current = openKey;
    setFocused(at);
  }, [openKey, rows]);

  // J and K move, and open what they reach while the panel is open; Enter opens, / searches, C adds; S, D and X change the
  // focused item: claimed or released, done, cancelled
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || modalOpen()) return;
      const el = event.target as HTMLElement | null;
      if (
        el &&
        (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable)
      )
        return;
      const at = rows[focused]?.item;
      const key = event.key.toLowerCase();
      if (event.key === "/") {
        event.preventDefault();
        document.querySelector<HTMLInputElement>(".work-items input[type=search]")?.focus();
      } else if ((key === "j" || key === "k") && rows.length) {
        // with the panel open they step from the open item, wherever the
        // pointer has moved the highlight, and the panel follows
        const opened = openKey ? rows.findIndex((r) => r.item.key === openKey) : -1;
        const from = opened >= 0 ? opened : focused;
        const next = key === "j" ? Math.min(from + 1, rows.length - 1) : Math.max(from - 1, 0);
        setFocused(next);
        if (openKey && rows[next].item.key !== openKey) openKeyed(rows[next].item.key);
      } else if (event.key === "Enter" && at) open(at);
      else if (key === "c" && !event.shiftKey) {
        event.preventDefault();
        setAdding(true);
      } else if (key === "s" && at) void change(at, { claim: at.status === "in_progress" ? "release" : "take" });
      else if (key === "d" && at) void change(at, { status: at.status === "done" ? "open" : "done" });
      else if (key === "x" && at) {
        // its open sub-items go with it: the person is asked first
        if (at.status !== "cancelled" && openSubItems(at) > 0) setCancelling(at);
        else void change(at, { status: at.status === "cancelled" ? "open" : "cancelled" });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [rows, focused, open, openKey, openKeyed, change]);

  const focusedItem = rows[focused]?.item;
  const row = ({ item, depth, folded: isFolded, children }: Shown, index: number, group: string) => {
    const closed = item.status === "done" || item.status === "cancelled";
    // a sub-item shown without its parent names it
    const orphan = item.parent && (!nested || !rows.some((r) => r.item.key === item.parent));
    return (
      <div
        key={item.key}
        role="button"
        tabIndex={-1}
        data-row={index}
        data-testid="item-row"
        data-key={item.key}
        className={`item-row ${index === focused ? "is-focused" : ""} ${item.key === openKey ? "is-open" : ""} ${closed ? "is-closed" : ""} ${dropOn === item.key ? "is-drop" : ""}`}
        style={{ paddingLeft: 10 + depth * 22 }}
        onMouseEnter={() => setFocused(index)}
        onClick={() => open(item)}
        draggable
        onDragStart={(event) => {
          event.dataTransfer.setData(DRAGGED, item.key);
          draggedGroup.current = group;
          event.dataTransfer.effectAllowed = "move";
          // after the drag has begun: a change to the page during
          // dragstart cancels the drag
          dragTimer.current = window.setTimeout(() => setDragging(item), 0);
        }}
        onDragOver={(event) => {
          if (!event.dataTransfer.types.includes(DRAGGED)) return;
          event.preventDefault();
          event.dataTransfer.dropEffect = "move";
          if (dropOn !== item.key) setDropOn(item.key);
        }}
        onDragLeave={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropOn(null);
        }}
        onDrop={(event) => {
          event.preventDefault();
          setDropOn(null);
          const dragged = event.dataTransfer.getData(DRAGGED);
          if (!dragged || dragged === item.key) return;
          // dropped on another item, it becomes its sub-item, in its repository
          const from = items?.find((i) => i.key === dragged);
          void (async () => {
            if (from && from.repo !== item.repo && !(await moveTo(dragged, item.repo))) return;
            await putUnder(dragged, item.key);
          })();
        }}
        onDragEnd={() => {
          window.clearTimeout(dragTimer.current);
          draggedGroup.current = null;
          setDropOn(null);
          setDragging(null);
        }}
      >
        <span className="item-fold">
          {children ? (
            <button
              type="button"
              aria-label={isFolded ? `Show the sub-items of ${item.key}` : `Hide the sub-items of ${item.key}`}
              aria-expanded={!isFolded}
              onClick={(event) => {
                event.stopPropagation();
                toggleFold(item.key);
              }}
            >
              {isFolded ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
            </button>
          ) : null}
        </span>
        <ItemStatusIcon status={item.status} />
        <span className="item-key mono">{item.key}</span>
        <span className="item-title">
          <span className="item-title-text">{item.title}</span>
          {orphan ? <span className="item-parent">in {item.parent}</span> : null}
          {layout !== "repository" && (layout === "list" || mixed.has(group)) ? (
            <span className="item-repo">{item.repo}</span>
          ) : null}
        </span>
        <span className="item-marks">
          {item.waiting ? (
            <span className="item-mark is-waiting" data-testid="item-waiting">
              <Clock size={11} aria-hidden="true" /> waiting on you
            </span>
          ) : null}
          {item.blocked && !closed ? (
            <span className="item-mark is-blocked" data-testid="item-blocked">
              <Lock size={11} aria-hidden="true" /> blocked by {item.blocked_by.join(", ")}
            </span>
          ) : null}
          {item.sub_items.total ? (
            <span
              className="item-progress"
              title={`${item.sub_items.closed} of ${item.sub_items.total} sub-items closed`}
            >
              {item.sub_items.closed}/{item.sub_items.total}
            </span>
          ) : null}
        </span>
        <span className="item-claim">
          {item.claim ? (
            item.claim.agent === "person" ? (
              <Tooltip label="You are working on it" side="top">
                <User size={13} aria-label="You" />
              </Tooltip>
            ) : (
              <Tooltip label={`${item.claim.agent} is working on it`} side="top">
                <AgentIcon requestedBy={item.claim.agent} size={14} />
              </Tooltip>
            )
          ) : null}
        </span>
        <time className="review-row-time" dateTime={item.updated_at} title={stamp(item.updated_at)}>
          {age(item.updated_at)}
        </time>
      </div>
    );
  };

  // the page's filters, in the Filter menu and as chips while they are on
  const clearFilters = () =>
    updateParams(
      (next) => {
        for (const key of ["q", "status", "repo", "project", "by", "waiting"]) next.delete(key);
      },
      { replace: true },
    );
  const filters: Filter[] = [
    {
      key: "status",
      label: "Status",
      value: status,
      off: "",
      choices: STATUSES.map((s) => ({ value: s.value, label: s.label, mark: statusIcon(s.value) })),
      onChange: (v) => setParam("status", v || null),
    },
    {
      key: "scope",
      label: "Project or repository",
      value: scope,
      off: "",
      searchable: true,
      choices: scopeOptions(
        live.namedProjects,
        [...new Set((items ?? []).map((i) => i.repo))].sort(),
        false,
        scope,
      ).map((o) => ({
        value: o.value,
        label: o.label,
        mark: o.project ? <ProjectAvatar project={o.project} size={14} /> : <FolderGit2 size={13} />,
      })),
      onChange: (v) => updateParams((next) => setScope(next, v), { replace: true }),
    },
    {
      key: "by",
      label: "Created by",
      value: by,
      off: "",
      choices: [
        { value: "", label: "Anyone" },
        { value: "person", label: "You" },
        ...agents.map((a) => ({ value: a, label: a, mark: <AgentIcon requestedBy={a} size={13} /> })),
      ],
      onChange: (v) => setParam("by", v || null),
    },
  ];
  const toggles: Toggle[] = [
    { key: "waiting", label: "Waiting on you", on: waiting, onChange: (on) => setParam("waiting", on ? "1" : null) },
  ];

  let index = -1;
  return (
    <div className={`work-items ${openKey ? "has-panel" : ""}`} ref={split}>
      <div className="work-items-side split-side">
        <header className="page-head">
          <div className="heading-inline">
            <h1>Work items</h1>
            {items ? <span className="inbox-total">{kept.length}</span> : null}
          </div>
          <div className="inbox-controls">
            <label className="search-field">
              <Search size={14} aria-hidden="true" />
              <input
                autoCorrect="off"
                autoCapitalize="off"
                type="search"
                autoComplete="off"
                spellCheck={false}
                placeholder="Search work items…"
                aria-label="Search work items"
                value={q}
                onChange={(e) => {
                  setQ(e.target.value);
                  setParam("q", e.target.value);
                }}
              />
            </label>
            <ListMenuButtons
              label="work items"
              filters={filters}
              toggles={toggles}
              onReset={clearFilters}
              displays={[
                groupingDisplay(layout, (next) => void update({ items: { layout: next } })),
                {
                  key: "ordering",
                  label: "Ordering",
                  value: prefs.items.ordering,
                  choices: ORDERINGS,
                  onChange: (next) => void update({ items: { ordering: next as Ordering } }),
                },
                {
                  key: "nesting",
                  label: "Sub-items",
                  value: prefs.items.nesting,
                  choices: [
                    { value: "nested", label: "Nested", icon: ListTree },
                    { value: "flat", label: "Flat", icon: ListIcon },
                  ],
                  onChange: (next) => void update({ items: { nesting: next as "nested" | "flat" } }),
                },
              ]}
            />
          </div>
        </header>
        <FilterChips filters={filters} toggles={toggles} onReset={clearFilters} />
        <QuickAdd
          open={adding}
          onOpen={() => setAdding(true)}
          onClose={() => setAdding(false)}
          repos={[...new Set([...repos, ...(items ?? []).map((i) => i.repo)])]}
          items={items ?? []}
          defaultRepo={params.get("repo") ?? focusedItem?.repo ?? null}
          defaultParent={focusedItem?.parent ?? null}
          onAdded={(item, more) => {
            reload();
            if (!more) {
              setAdding(false);
              open(item);
            }
          }}
        />
        {!items ? null : kept.length === 0 ? (
          <EmptyState
            title={filtered ? "No matching work items" : "No work items yet"}
            icon={filtered ? <SearchX size={28} strokeWidth={1.5} /> : <ListChecks size={28} strokeWidth={1.5} />}
          >
            {filtered ? (
              "Try a different search or clear the filters."
            ) : (
              <>
                Press <kbd>C</kbd> to add one, or ask an agent to plan a task with <code>pinrail items add</code>.
              </>
            )}
          </EmptyState>
        ) : (
          <div className="inbox-groups item-groups" ref={list}>
            {dragging?.parent ? (
              <div
                className={`item-top-drop ${dropOn === "" ? "is-drop" : ""}`}
                data-testid="item-top-drop"
                onDragOver={(event) => {
                  if (!event.dataTransfer.types.includes(DRAGGED)) return;
                  event.preventDefault();
                  event.dataTransfer.dropEffect = "move";
                  if (dropOn !== "") setDropOn("");
                }}
                onDragLeave={() => setDropOn(null)}
                onDrop={(event) => {
                  event.preventDefault();
                  setDropOn(null);
                  const dragged = event.dataTransfer.getData(DRAGGED);
                  if (dragged) void putUnder(dragged, null, dragging.parent);
                }}
              >
                Drop here to move it to the top level
              </div>
            ) : null}
            {shownGroups.map(([key, shown]) =>
              layout === "list" ? (
                <div key="list" className="inbox-list">
                  {shown.map((s) => row(s, ++index, key))}
                </div>
              ) : (
                <details key={key || "none"} className="inbox-repo" open data-testid="inbox-group">
                  <summary
                    data-testid="inbox-group-heading"
                    className={
                      dragging && key !== draggedGroup.current && reposOf(key).length
                        ? `is-drop-target ${dropOn === `group:${key}` ? "is-drop" : ""}`
                        : undefined
                    }
                    onDragOver={(event) => {
                      if (!event.dataTransfer.types.includes(DRAGGED)) return;
                      if (key === draggedGroup.current || !reposOf(key).length) return;
                      event.preventDefault();
                      event.dataTransfer.dropEffect = "move";
                      if (dropOn !== `group:${key}`) setDropOn(`group:${key}`);
                    }}
                    onDragLeave={(event) => {
                      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropOn(null);
                    }}
                    onDrop={(event) => {
                      event.preventDefault();
                      setDropOn(null);
                      const dragged = event.dataTransfer.getData(DRAGGED);
                      if (dragged && key !== draggedGroup.current) dropOnGroup(key, event.currentTarget, dragged);
                    }}
                  >
                    <GroupHeading groupKey={key} project={shown[0]?.item.project} count={shown.length} />
                  </summary>
                  {shown.map((s) => row(s, ++index, key))}
                </details>
              ),
            )}
          </div>
        )}
        <footer className="inbox-footer">
          <KeyHints
            hints={[
              { keys: ["J / K"], label: "Move" },
              { keys: ["Enter"], label: "Open" },
              { keys: ["C"], label: "Add" },
              { keys: ["/"], label: "Search" },
              { keys: ["S"], label: "Claim" },
              { keys: ["D"], label: "Done" },
              { keys: ["X"], label: "Cancel" },
            ]}
          />
        </footer>
      </div>
      {choosing ? (
        <RepoChoiceMenu
          anchor={choosing.anchor}
          repos={choosing.repos}
          onPick={(repo) => {
            void moveTo(choosing.key, repo);
            setChoosing(null);
          }}
          onClose={() => setChoosing(null)}
        />
      ) : null}
      {cancelling ? (
        <CancelItemDialog
          item={cancelling}
          onClose={() => setCancelling(null)}
          onConfirm={() => {
            void change(cancelling, { status: "cancelled" });
            setCancelling(null);
          }}
        />
      ) : null}
      {openKey ? (
        <SidePanel split={split} label="Work item">
          <ItemPanel
            key={openKey}
            itemKey={openKey}
            items={items ?? []}
            repos={[...new Set([...repos, ...(items ?? []).map((i) => i.repo)])]}
            onOpen={openKeyed}
            onClose={closePanel}
            onChanged={reload}
          />
        </SidePanel>
      ) : null}
    </div>
  );
}

/** The one quick-add control at the top of the list: a row that opens into
 *  a field with the repository and the parent the item goes in. */
function QuickAdd({ open, onOpen, ...box }: { open: boolean; onOpen: () => void } & BoxProps) {
  if (!open)
    return (
      <button type="button" className="item-quick-add" onClick={onOpen} data-testid="item-quick-add">
        <Plus size={14} aria-hidden="true" /> Add work item <kbd>C</kbd>
      </button>
    );
  return <QuickAddBox {...box} />;
}

type BoxProps = {
  onClose: () => void;
  repos: string[];
  items: WorkItem[];
  defaultRepo: string | null;
  defaultParent: string | null;
  onAdded: (item: WorkItem, more: boolean) => void;
};

/** The open quick add. It is made each time it opens, starting from the
 *  list's current place, so nothing resets it after the person starts typing. */
function QuickAddBox({ onClose, repos, items, defaultRepo, defaultParent, onAdded }: BoxProps) {
  const toast = useToast();
  const sorted = useMemo(() => [...repos].sort(), [repos]);
  const [title, setTitle] = useState("");
  const [repo, setRepo] = useState(() => defaultRepo ?? [...repos].sort()[0] ?? "");
  const [parent, setParent] = useState(() => defaultParent ?? "");
  const [busy, setBusy] = useState(false);
  const field = useRef<HTMLInputElement>(null);
  // focused before it is shown, so no key typed after C goes elsewhere
  useLayoutEffect(() => {
    field.current?.focus();
  }, []);

  const parents = items.filter((i) => i.repo === repo && i.status !== "done" && i.status !== "cancelled");

  const add = async (more: boolean) => {
    if (!title.trim() || !repo || busy) return;
    setBusy(true);
    try {
      const item = await api.addItem({ repo, title: title.trim(), ...(parent ? { parent } : {}) });
      setTitle("");
      onAdded(item, more);
    } catch (error) {
      toast(reason(error), "danger");
    } finally {
      setBusy(false);
      field.current?.focus();
    }
  };

  const onKey = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void add(event.metaKey || event.ctrlKey);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    } else if (event.key === "Tab" && event.shiftKey && sorted.length) {
      event.preventDefault();
      setRepo(sorted[(sorted.indexOf(repo) + 1) % sorted.length]);
      setParent("");
    }
  };

  return (
    <div className="item-quick-box" data-testid="item-quick-box">
      <div className="item-quick-line">
        <div className="item-quick-main">
          <ItemStatusIcon status="open" />
          <input
            ref={field}
            className="item-quick-title"
            aria-label="What needs to be done?"
            placeholder="What needs to be done?"
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck={false}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onKeyDown={onKey}
          />
        </div>
        <div className="item-quick-controls">
          <Select
            id="item-quick-repo"
            searchable
            label="Repository"
            icon={<FolderGit2 size={13} />}
            value={repo}
            onChange={(v) => {
              setRepo(v);
              setParent("");
            }}
            options={sorted.map((r) => ({ value: r, label: r }))}
          />
          <Select
            id="item-quick-parent"
            searchable
            label="Parent"
            value={parent}
            onChange={setParent}
            options={[
              { value: "", label: "No parent" },
              ...parents.map((p) => ({ value: p.key, label: `${p.key} ${p.title}` })),
            ]}
          />
          <Tooltip label="Add it" side="bottom">
            <button
              type="button"
              className="item-quick-action is-add"
              aria-label="Add it"
              disabled={!title.trim() || !repo || busy}
              onClick={() => void add(false)}
            >
              <Plus size={14} />
            </button>
          </Tooltip>
          <Tooltip label="Cancel" side="bottom">
            <button type="button" className="item-quick-action" aria-label="Cancel" onClick={onClose}>
              <X size={14} />
            </button>
          </Tooltip>
        </div>
      </div>
      <div className="item-quick-keys">
        <KeyHints
          hints={[
            { keys: ["Enter"], label: "Add and open" },
            { keys: [MOD, "Enter"], label: "Add and add more" },
            { keys: [SHIFT, "Tab"], label: "Change repository" },
            { keys: ["Esc"], label: "Cancel" },
          ]}
        />
        {repos.length === 0 ? (
          <span className="item-quick-note">Name a repository: an item belongs to one.</span>
        ) : null}
      </div>
    </div>
  );
}

/** The repositories of a project an item was dropped on, to pick the one
 *  it moves to. */
function RepoChoiceMenu({
  anchor,
  repos,
  onPick,
  onClose,
}: {
  anchor: HTMLElement;
  repos: string[];
  onPick: (repo: string) => void;
  onClose: () => void;
}) {
  const { menu, pos } = useOptionsMenu(anchor, onClose);
  // above the heading when it does not fit under it
  const [above, setAbove] = useState<number | null>(null);
  useLayoutEffect(() => {
    const el = menu.current;
    if (!el || !pos) return;
    const height = el.getBoundingClientRect().height;
    if (pos.top + height > window.innerHeight - 8) setAbove(Math.max(8, pos.top - height - 8 - anchor.offsetHeight));
  }, [menu, pos, anchor]);
  if (!pos) return null;
  return createPortal(
    <div
      ref={menu}
      className="menu-popup"
      role="menu"
      aria-label="Move to which repository?"
      style={{ top: above ?? pos.top, left: pos.left }}
    >
      <div className="menu-heading">Move to which repository?</div>
      {repos.map((repo) => (
        <button key={repo} type="button" role="menuitem" className="menu-item" onClick={() => onPick(repo)}>
          <FolderGit2 size={14} className="menu-icon" aria-hidden />
          {repo}
        </button>
      ))}
    </div>,
    document.body,
  );
}
