// Work items: what the person and agents record and work through, as one
// list grouped like the inbox, with sub-items nested under their parent or
// each as a row of its own, a quick add at the top, and filters.

import { FolderGit2, ListChecks, Lock, Plus, Search, SearchX } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, errorMessage } from "../api/client";
import type { WorkItem } from "../api/types";
import { AgentIcon } from "../components/AgentIcon";
import { EmptyState } from "../components/EmptyState";
import { GroupHeading } from "../components/GroupHeading";
import { ItemStatusIcon } from "../components/ItemStatusIcon";
import { FilterChips, ListMenuButtons, groupingDisplay, type Filter, type Toggle } from "../components/ListMenus";
import { ORDERINGS, orderItems, type Ordering } from "../lib/itemOrder";
import { List as ListIcon, ListTree } from "lucide-react";
import { ProjectAvatar } from "../components/ProjectAvatar";
import { groupKey, grouped, mixedGroups } from "../lib/grouping";
import { hasMod, modalOpen, isTyping } from "../lib/keys";
import { KeyHints } from "../components/KeyHints";
import { QuickAddBox, type QuickAddBoxProps } from "../components/QuickAddBox";
import { ItemPanel } from "../components/ItemPanel";
import { ItemRow, type ItemRowActions } from "../components/ItemRow";
import { CancelItemDialog, openSubItems } from "../components/CancelItemDialog";
import { DeleteItemDialog } from "../components/DeleteItemDialog";
import { HoldItemDialog, stoppedByHold } from "../components/HoldItemDialog";
import { SidePanel } from "../components/SidePanel";
import { PopoverMenu } from "../components/PopoverMenu";
import { inScope, scopeOf, scopeOptions, setScope, useUrlParams } from "../lib/url";
import { useLive, aboutItems, useNoticeCount } from "../state/live";
import { useSettings } from "../state/settings";
import { useToast } from "../state/toasts";
import { useWindowKey } from "../lib/useWindowKey";
import { useListKeys } from "../lib/useListKeys";
import { useSlashSearch } from "../lib/useSlashSearch";
import { useDebounced } from "../lib/useDebounced";
import { itemTree, type ItemTreeRow } from "../lib/itemTree";
import { useDragDrop } from "../lib/useDragDrop";
import { useLatest } from "../lib/useLatest";
import { useLoad } from "../lib/useLoad";

/** How long the search waits for typing to pause before it asks the server. */
const SEARCH_DELAY_MS = 250;

/** The choices of the status filter, and what each asks the core for. */
const STATUSES: { value: string; label: string; query?: string }[] = [
  { value: "", label: "Open", query: "open,in_progress,on_hold" },
  { value: "ready", label: "Ready", query: "ready" },
  { value: "blocked", label: "Blocked", query: "blocked" },
  { value: "in_progress", label: "In progress", query: "in_progress" },
  { value: "open", label: "Not started", query: "open" },
  { value: "on_hold", label: "On hold", query: "on_hold" },
  { value: "done", label: "Done", query: "done" },
  { value: "cancelled", label: "Cancelled", query: "cancelled" },
  { value: "all", label: "All", query: "open,in_progress,on_hold,done,cancelled" },
];

/** The icon of a choice of the status filter: the status's own, or what
 *  marks a blocked item. */
function statusIcon(value: string) {
  if (value === "blocked") return <Lock size={13} className="item-status-blocked" aria-hidden="true" />;
  if (value === "ready" || value === "open") return <ItemStatusIcon status="open" decorative />;
  if (value === "in_progress" || value === "on_hold" || value === "done" || value === "cancelled")
    return <ItemStatusIcon status={value} decorative />;
  return <ListChecks size={13} aria-hidden="true" />;
}

/** What a dragged row carries: its item's key. */
const DRAGGED = "application/x-pinrail-item";

/** An item in the order J and K walk, at its depth in the tree. */
type Shown = ItemTreeRow;

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
  // each project's repositories, by its id: where an item dropped on it goes
  const [folded, setFolded] = useState<Set<string>>(new Set());
  // the page reloads after its own changes, whether or not an event brings them
  const [reloads, setReloads] = useState(0);
  const reload = useCallback(() => setReloads((n) => n + 1), []);

  const debounced = useDebounced(query, SEARCH_DELAY_MS);
  // the notices that can change the items, which load them again
  const itemNotices = useNoticeCount(aboutItems);
  const { data: items, error: loadError } = useLoad(
    () =>
      api
        .items({ status: STATUSES.find((s) => s.value === status)?.query, q: debounced || undefined })
        .then((d) => d.items),
    [status, debounced, itemNotices, reloads],
  );
  // the person's queue, in its order: the items agents take first
  const [queue, setQueue] = useState<WorkItem[]>([]);
  useEffect(() => {
    let cancelled = false;
    api
      .items({ queued: "true" })
      .then((d) => !cancelled && setQueue(d.items))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [itemNotices, reloads]);
  // the repositories an item can be added to: every one the app knows
  const listing = live.projectsListing;
  const repos = useMemo(
    () =>
      listing
        ? [
            ...new Set([
              ...listing.projects.flatMap((p) => p.repos.map((r) => r.repo)),
              ...listing.loose.map((r) => r.repo),
            ]),
          ]
        : [],
    [listing],
  );
  const projectRepos = useMemo<Record<string, string[]>>(
    () => Object.fromEntries((listing?.projects ?? []).map((p) => [p.id, p.repos.map((r) => r.repo)])),
    [listing],
  );

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

  const shownGroups = useMemo(
    () => groups.map(([key, list]) => [key, itemTree(list, { nested, folded })] as [string, Shown[]]),
    [groups, nested, folded],
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
        toast(errorMessage(error), "danger");
      }
      reload();
    },
    [toast, reload],
  );

  /** Changes the queue, and shows the order the core answers. */
  const changeQueue = useCallback(
    async (run: () => Promise<{ items: WorkItem[] }>) => {
      try {
        setQueue((await run()).items);
      } catch (error) {
        toast(errorMessage(error), "danger");
      }
      reload();
    },
    [toast, reload],
  );
  const toggleQueue = useCallback(
    (item: WorkItem) => void changeQueue(() => (item.queue ? api.unqueueItem(item.key) : api.queueItem(item.key))),
    [changeQueue],
  );
  /** Moves a queued item one place up, or down. */
  const moveInQueue = useCallback(
    (item: WorkItem, by: -1 | 1) => {
      const at = queue.findIndex((q) => q.key === item.key);
      const other = at < 0 ? undefined : queue[at + by];
      if (!other) return;
      void changeQueue(() => api.queueItem(item.key, by < 0 ? { before: other.key } : { after: other.key }));
    },
    [queue, changeQueue],
  );

  // an item with open sub-items waiting for the person to confirm its cancel
  const [cancelling, setCancelling] = useState<WorkItem | null>(null);
  // an item waiting for the person to confirm its deletion
  const [deleting, setDeleting] = useState<WorkItem | null>(null);
  // an item about to go on hold with work in progress under it, and that work
  const [holding, setHolding] = useState<{ item: WorkItem; stopped: WorkItem[] } | null>(null);
  /** Puts an item on hold, asking first when it stops work in progress. */
  const hold = useCallback(
    (item: WorkItem) =>
      void stoppedByHold(item)
        .then((stopped) => {
          if (stopped.length) setHolding({ item, stopped });
          else void change(item, { status: "on_hold" });
        })
        .catch(() => {}),
    [change],
  );
  // an item dragged onto another becomes its sub-item, and a sub-item
  // dragged to the top of the list leaves its parent
  const drag = useDragDrop(DRAGGED);
  const dropOn = drag.dropOn;
  // the item dragged, and the group it is in, which is no place to drop it
  const [dragging, setDragging] = useState<{ item: WorkItem; group: string } | null>(null);
  const dragTimer = useRef(0);
  const putUnder = useCallback(
    async (key: string, parent: string | null, from?: string | null) => {
      try {
        await api.editItem(key, { parent });
        toast(parent ? `${key} is now part of ${parent}` : `${key} is no longer part of ${from}`);
      } catch (error) {
        toast(errorMessage(error), "danger");
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
        toast(errorMessage(error), "danger");
        return false;
      } finally {
        reload();
      }
    },
    [toast, reload],
  );
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

  // J, K and Enter, and walking the side panel, as in the other lists
  const search = useRef<HTMLInputElement>(null);
  const { focused, hover } = useListKeys({
    items: rows,
    keyOf: (r) => r.item.key,
    openKey,
    onOpen: (r) => open(r.item),
    onWalk: (r) => openKeyed(r.item.key),
    rowOf: (i) => document.querySelector<HTMLElement>(`[data-testid="item-row"][data-row="${i}"]`),
  });
  useSlashSearch(search);

  /** S, D, Q, H, X and ⌘⌫ on the item open in the panel, which may have
   *  left the list, or else on the focused row. */
  const act = async (key: string) => {
    let at = openKey ? items?.find((i) => i.key === openKey) : rows[focused]?.item;
    if (openKey && !at) at = await api.item(openKey).catch(() => undefined);
    if (key === "delete" && at) setDeleting(at);
    else if (key === "s" && at) void change(at, { claim: at.status === "in_progress" ? "release" : "take" });
    else if (key === "d" && at) void change(at, { status: at.status === "done" ? "open" : "done" });
    else if (key === "q" && at && at.status !== "done" && at.status !== "cancelled") toggleQueue(at);
    else if (key === "h" && at && at.status !== "done" && at.status !== "cancelled") {
      if (at.status === "on_hold") void change(at, { status: "open" });
      else hold(at);
    } else if (key === "x" && at) {
      // its open sub-items go with it: the person is asked first
      if (at.status !== "cancelled" && openSubItems(at) > 0) setCancelling(at);
      else void change(at, { status: at.status === "cancelled" ? "open" : "cancelled" });
    }
  };
  // J and K move, and open what they reach while the panel is open; Enter opens, / searches, C adds; S, D and X change the
  // open or the focused item: claimed or released, done, cancelled; ⌘⌫ deletes it, after asking
  useWindowKey((event) => {
    const typing = (el: HTMLElement | null) => isTyping(el);
    // ⇧↑ and ⇧↓ move the focused item in the queue; ⌥↑ and ⌥↓ go to the
    // waiting reviews, everywhere
    if (
      event.shiftKey &&
      !event.altKey &&
      !event.metaKey &&
      !event.ctrlKey &&
      (event.key === "ArrowUp" || event.key === "ArrowDown") &&
      !modalOpen() &&
      !typing(event.target as HTMLElement | null)
    ) {
      const at = rows[focused]?.item;
      if (at?.queue) {
        event.preventDefault();
        moveInQueue(at, event.key === "ArrowUp" ? -1 : 1);
      }
      return;
    }
    // ⌘⌫ in a text field deletes text, not the item
    if (
      hasMod(event) &&
      !event.shiftKey &&
      !event.altKey &&
      (event.key === "Backspace" || event.key === "Delete") &&
      !modalOpen() &&
      !typing(event.target as HTMLElement | null)
    ) {
      event.preventDefault();
      void act("delete");
      return;
    }
    if (event.metaKey || event.ctrlKey || event.altKey || modalOpen()) return;
    const el = event.target as HTMLElement | null;
    if (isTyping(el)) return;
    const key = event.key.toLowerCase();
    if (key === "c" && !event.shiftKey) {
      event.preventDefault();
      setAdding(true);
    } else if ("sdqhx".includes(key) && key.length === 1) void act(key);
  });
  // ⌘⇧C, which elsewhere opens the quick add dialog, opens this page's own
  useEffect(() => {
    const onQuickAdd = () => setAdding(true);
    window.addEventListener("pinrail:quick-add", onQuickAdd);
    return () => window.removeEventListener("pinrail:quick-add", onQuickAdd);
  }, []);

  const focusedItem = rows[focused]?.item;
  // What a row does, as the same functions from one render to the next, so
  // that a memoized row draws again only when its own props change; each
  // calls the page's latest state.
  const rowHandlers: ItemRowActions = {
    hover: (index) => hover(index),
    open: (item) => open(item),
    toggleFold: (key) => toggleFold(key),
    dragStart: (event, item, group) => {
      drag.start(event, item.key);
      // after the drag has begun: a change to the page during dragstart
      // cancels the drag
      dragTimer.current = window.setTimeout(() => setDragging({ item, group }), 0);
    },
    dragOver: (event, item) => drag.over(event, item.key),
    dragLeave: drag.leave,
    drop: (event, item) => {
      const dragged = drag.take(event);
      if (!dragged || dragged === item.key) return;
      // dropped on another item, it becomes its sub-item, and stays in its
      // own repository, with its key
      void putUnder(dragged, item.key);
    },
    dragEnd: () => {
      window.clearTimeout(dragTimer.current);
      drag.end();
      setDragging(null);
    },
  };
  const latestRow = useLatest(rowHandlers);
  const rowActions = useMemo<ItemRowActions>(
    () => ({
      hover: (index) => latestRow.current.hover(index),
      open: (item) => latestRow.current.open(item),
      toggleFold: (key) => latestRow.current.toggleFold(key),
      dragStart: (event, item, group) => latestRow.current.dragStart(event, item, group),
      dragOver: (event, item) => latestRow.current.dragOver(event, item),
      dragLeave: (event) => latestRow.current.dragLeave(event),
      drop: (event, item) => latestRow.current.drop(event, item),
      dragEnd: () => latestRow.current.dragEnd(),
    }),
    [latestRow],
  );
  const row = ({ item, depth, folded: isFolded, children }: Shown, index: number, group: string) => (
    <ItemRow
      key={item.key}
      item={item}
      depth={depth}
      folded={isFolded}
      children={children}
      index={index}
      group={group}
      focused={index === focused}
      open={item.key === openKey}
      dropTarget={dropOn === item.key}
      // a sub-item not shown under its parent names it: flat, filtered
      // apart from it, or in another repository's group than it
      orphanOf={item.parent && depth === 0 ? item.parent : null}
      showRepo={layout !== "repository" && (layout === "list" || mixed.has(group))}
      actions={rowActions}
    />
  );

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
                ref={search}
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
        {loadError ? <p className="notice notice-danger">{loadError}</p> : null}
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
          <div className="inbox-groups item-groups">
            {dragging?.item.parent ? (
              <div
                className={`item-top-drop ${dropOn === "" ? "is-drop" : ""}`}
                data-testid="item-top-drop"
                {...drag.target("", (dragged) => void putUnder(dragged, null, dragging.item.parent))}
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
                      dragging && key !== dragging.group && reposOf(key).length
                        ? `is-drop-target ${dropOn === `group:${key}` ? "is-drop" : ""}`
                        : undefined
                    }
                    {...drag.target(
                      `group:${key}`,
                      (dragged, event) => dropOnGroup(key, event.currentTarget as HTMLElement, dragged),
                      !!dragging && key !== dragging.group && reposOf(key).length > 0,
                    )}
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
              { keys: ["H"], label: "Hold" },
              { keys: ["Q"], label: "Queue" },
            ]}
          />
        </footer>
      </div>
      {choosing ? (
        <PopoverMenu
          anchor={choosing.anchor}
          label="Move to which repository?"
          entries={[
            { kind: "heading", label: "Move to which repository?" },
            ...choosing.repos.map((repo) => ({
              label: repo,
              icon: FolderGit2,
              onPick: () => void moveTo(choosing.key, repo),
            })),
          ]}
          onClose={() => setChoosing(null)}
        />
      ) : null}
      {holding ? (
        <HoldItemDialog
          item={holding.item}
          stopped={holding.stopped}
          onClose={() => setHolding(null)}
          onConfirm={() => {
            void change(holding.item, { status: "on_hold" });
            setHolding(null);
          }}
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
      {deleting ? (
        <DeleteItemDialog
          item={deleting}
          onClose={() => setDeleting(null)}
          onDone={() => {
            if (deleting.key === openKey) closePanel();
            setDeleting(null);
            reload();
          }}
        />
      ) : null}
      {openKey ? (
        <SidePanel split={split} label="Work item" listMin={560}>
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
function QuickAdd({ open, onOpen, ...box }: { open: boolean; onOpen: () => void } & QuickAddBoxProps) {
  if (!open)
    return (
      <button type="button" className="item-quick-add" onClick={onOpen} data-testid="item-quick-add">
        <Plus size={14} aria-hidden="true" /> Add work item <kbd>C</kbd>
      </button>
    );
  return <QuickAddBox {...box} />;
}
