// Work items: what the person and agents record and work through, as one
// list grouped like the inbox, with sub-items nested under their parent or
// each as a row of its own, a quick add at the top, and filters.

import { ChevronDown, FolderGit2, Heading, ListChecks, Lock, Plus, SearchX, TextSearch } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api, errorMessage } from "../api/client";
import type { FoundItem, WorkItem } from "../api/types";
import { AgentIcon } from "../components/AgentIcon";
import { EmptyState } from "../components/EmptyState";
import { GroupHeading } from "../components/GroupHeading";
import { ItemStatusIcon } from "../components/ItemStatusIcon";
import { Tooltip } from "../components/Tooltip";
import { FilterChips, ListMenuButtons, groupingDisplay, type Filter, type Toggle } from "../components/ListMenus";
import { ORDERINGS, orderItems, type Ordering } from "../lib/itemOrder";
import { ChevronsDownUp, ChevronsUpDown, List as ListIcon, ListTree } from "lucide-react";
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
import { SearchInput } from "../components/SearchInput";
import { PopoverMenu } from "../components/PopoverMenu";
import { NO_PROJECT, inScope, scopeOf, scopeOptions, setScope, useUrlParams } from "../lib/url";
import { useLive, aboutItems, useNoticeCount } from "../state/live";
import { useSettings } from "../state/settings";
import { useToast } from "../state/toasts";
import { useWindowKey } from "../lib/useWindowKey";
import { useListKeys } from "../lib/useListKeys";
import { useSlashSearch } from "../lib/useSlashSearch";
import { useDebounced } from "../lib/useDebounced";
import { itemTree, pageOfGroups, withClosedSubItems, type ItemTreeRow } from "../lib/itemTree";
import { Pager, pageOf, pageSizeOf } from "../components/Pager";
import { useDragDrop } from "../lib/useDragDrop";
import { useLatest } from "../lib/useLatest";
import { useLoad } from "../lib/useLoad";
import { readStored, writeStored } from "../lib/storage";

/** What the search looks in: the titles, or everything an item holds. */
const SEARCH_MODES = {
  title: { label: "Titles", hint: "Strict", icon: Heading, tip: "Search titles only" },
  text: { label: "Full text", hint: "Ranked", icon: TextSearch, tip: "Search titles, descriptions and comments" },
} as const;
type SearchMode = keyof typeof SEARCH_MODES;
/** The group of a full-text search's closed matches. */
const CLOSED_GROUP = "closed";
/** How long the search waits for typing to pause before it asks the server. */
const SEARCH_DELAY_MS = 250;
/** The most items a full-text search lists, of the open and of the closed ones each. */
const FOUND_LIMIT = 100;
/** Whether an item is closed, for the full-text search that lists those last. */
const ended = (i: WorkItem) => i.status === "done" || i.status === "cancelled";

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

/** How many top-level items a page shows, each with its sub-items, unless
 *  the person chooses another size. */
const ITEMS_PER_PAGE = 100;

/** What a dragged row carries: its item's key. */
const DRAGGED = "application/x-pinrail-item";

/** An item in the order J and K walk, at its depth in the tree. */
type Shown = ItemTreeRow;

/** Where the parents the person folded or unfolded are kept, and how many:
 *  the oldest go first. */
const FOLDED = "pinrail.items.folded";
const MAX_FOLDED = 500;

/** The parents the person folded (true) or unfolded (false), by key. */
function storedFolds(): Map<string, boolean> {
  try {
    const entries: unknown = JSON.parse(readStored(FOLDED) ?? "[]");
    if (!Array.isArray(entries)) return new Map();
    return new Map(
      entries.filter(
        (e): e is [string, boolean] => Array.isArray(e) && typeof e[0] === "string" && typeof e[1] === "boolean",
      ),
    );
  } catch {
    return new Map();
  }
}

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
  // the done and cancelled sub-items of the parents listed, under them
  const closedSubs = params.get("closed") === "1";
  const openKey = params.get("open");
  const filtered = !!(query || status || scope || by || waiting || closedSubs);
  // each project's repositories, by its id: where an item dropped on it goes
  // the parents the person folded or unfolded, kept on this machine; the
  // others are as they start
  const [folded, setFoldedState] = useState(storedFolds);
  /** Folds (true) or unfolds (false) parents, by key. */
  const setFolds = useCallback((keys: string[], fold: boolean) => {
    setFoldedState((f) => {
      const next = new Map(f);
      for (const key of keys) {
        // the latest last, so that the oldest go first
        next.delete(key);
        next.set(key, fold);
      }
      writeStored(FOLDED, JSON.stringify([...next].slice(-MAX_FOLDED)));
      return next;
    });
  }, []);
  // the page reloads after its own changes, whether or not an event brings them
  const [reloads, setReloads] = useState(0);
  const reload = useCallback(() => setReloads((n) => n + 1), []);

  const debounced = useDebounced(query, SEARCH_DELAY_MS);
  // titles only, or the full text ranked by how well it matches
  const searchMode = prefs.items.search;
  const ModeIcon = SEARCH_MODES[searchMode].icon;
  // the search mode's menu, open under its button
  const [modeMenu, setModeMenu] = useState<HTMLElement | null>(null);
  const ranked = searchMode === "text" && !!debounced.trim();
  // a full-text search asks only for the matches in the repository or
  // project chosen; the items without a repository the list keeps itself
  const repoParam = (ranked && params.get("repo") !== NO_PROJECT && params.get("repo")) || undefined;
  const projectParam = (ranked && params.get("project")) || undefined;
  // the notices that can change the items, which load them again
  const itemNotices = useNoticeCount(aboutItems);
  // a status that already lists the closed items has no closed sub-items to add
  const addClosed = closedSubs && !["done", "cancelled", "all"].includes(status);
  const { data: loaded, error: loadError } = useLoad(async () => {
    const q = debounced || undefined;
    const statusQuery = STATUSES.find((s) => s.value === status)?.query;
    if (ranked) {
      // the best matches first, in the repository or project chosen; with
      // the statuses open by default, the closed items that match follow
      // the open ones
      const within = { q: debounced, repo: repoParam, project: projectParam, limit: FOUND_LIMIT };
      const [listed, closed] = await Promise.all([
        api.searchItems({ ...within, status: statusQuery }),
        status === "" ? api.searchItems({ ...within, status: "done,cancelled" }) : Promise.resolve({ items: [] }),
      ]);
      const found: FoundItem[] = [...listed.items, ...closed.items];
      return {
        items: found as WorkItem[],
        next: new Set<string>(),
        found: new Map(found.map((f) => [f.key, f])),
      };
    }
    const [listed, closed] = await Promise.all([
      api.items({ status: statusQuery, q, in: searchMode === "title" ? "title" : undefined }),
      addClosed ? api.items({ status: "done,cancelled", q }).then((d) => d.items) : Promise.resolve([]),
    ]);
    return { items: withClosedSubItems(listed.items, closed), next: new Set(listed.next), found: null };
  }, [status, debounced, itemNotices, reloads, addClosed, ranked, searchMode, repoParam, projectParam]);
  const items = loaded?.items;
  // what a full-text search found in each item, by key
  const found = loaded?.found ?? null;
  // each repository's item that pinrail items next gives an agent
  const nextKeys = loaded?.next;
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

  // a filter changed starts again at the first page
  const setParam = (key: string, value: string | null) =>
    updateParams(
      (next) => {
        if (value) next.set(key, value);
        else next.delete(key);
        next.delete("page");
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
  // the closed sub-items added come after their open siblings
  // a full-text search's matches stay in the order it ranked them
  const ordered = useMemo(() => {
    const list = found ? kept : orderItems(kept, prefs.items.ordering, { nested });
    if (!addClosed && !found) return list;
    return [...list.filter((i) => !ended(i)), ...list.filter(ended)];
  }, [kept, prefs.items.ordering, nested, addClosed, found]);
  // ranked, the matches are one list, with the closed ones under a heading
  const groups = useMemo(
    () =>
      found
        ? ([
            ["", ordered.filter((i) => !ended(i))],
            [CLOSED_GROUP, ordered.filter(ended)],
          ].filter(([, list]) => list.length) as [string, WorkItem[]][])
        : layout === "list"
          ? [["", ordered] as [string, WorkItem[]]]
          : grouped(ordered, (i) => groupKey(layout, i.repo, i.project)),
    [ordered, layout, found],
  );
  // a done or cancelled parent starts folded, with the option on
  const foldClosed = prefs.items.fold_closed;
  const shownFolded = useMemo(() => {
    const shown = new Set<string>();
    for (const i of ordered) {
      const starts = foldClosed && (i.status === "done" || i.status === "cancelled");
      if (folded.get(i.key) ?? starts) shown.add(i.key);
    }
    return shown;
  }, [ordered, foldClosed, folded]);
  // the parents in the list, which Fold all folds
  const parents = useMemo(() => {
    if (!nested) return [];
    const keys = new Set(ordered.map((i) => i.key));
    return [...new Set(ordered.flatMap((i) => (i.parent && keys.has(i.parent) ? [i.parent] : [])))];
  }, [ordered, nested]);
  const allFolded = parents.length > 0 && parents.every((key) => shownFolded.has(key));
  /** Folds every parent, or unfolds them all when every one is folded. */
  const foldAll = () => setFolds(parents, !allFolded);
  const mixed = useMemo(() => mixedGroups(groups, (i) => i.repo), [groups]);

  const shownGroups = useMemo(
    () =>
      groups.map(
        ([key, list]) => [key, itemTree(list, { nested: nested && !found, folded: shownFolded })] as [string, Shown[]],
      ),
    [groups, nested, shownFolded, found],
  );
  // one page of the top-level items, each with its sub-items: drawing
  // thousands of rows at once makes the page slow to show and to use
  const size = pageSizeOf(params.get("per"), ITEMS_PER_PAGE);
  const topLevel = useMemo(
    () => shownGroups.reduce((n, [, r]) => n + r.filter((row) => row.depth === 0).length, 0),
    [shownGroups],
  );
  const pages = Math.max(1, Math.ceil(topLevel / size));
  // a page past the end, after a filter or a change shrank the list: the last
  const page = Math.min(pageOf(params.get("page")), pages);
  const pageGroups = useMemo(() => pageOfGroups(shownGroups, page, size).groups, [shownGroups, page, size]);
  const rows = useMemo(() => pageGroups.flatMap(([, r]) => r), [pageGroups]);
  // each group's whole count, for its heading on any page
  const groupTotals = useMemo(() => new Map(shownGroups.map(([key, r]) => [key, r.length])), [shownGroups]);
  const agents = useMemo(
    () => [...new Set((items ?? []).map((i) => i.created_by).filter((b) => b !== "person"))].sort(),
    [items],
  );

  /** A page number, or a step from the page the URL is on. */
  const setPage = (n: number | ((current: number) => number)) => {
    updateParams((next) => {
      const now = Math.min(pageOf(next.get("page")), pages);
      const value = Math.min(Math.max(1, typeof n === "function" ? n(now) : n), pages);
      if (value > 1) next.set("page", String(value));
      else next.delete("page");
    });
    window.scrollTo({ top: 0 });
  };

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
  // Folding keeps the parent where it was on the screen. Beside the side
  // panel the list scrolls on its own, and WebKit puts it back at its top
  // as the rows under the parent come or go, so the list is scrolled to
  // keep the parent's row in place once the change is drawn, and again on
  // the next frame.
  const foldPlace = useRef<{ key: string; top: number } | null>(null);
  const toggleFold = (key: string) => {
    const row = document.querySelector<HTMLElement>(`[data-testid="item-row"][data-key="${CSS.escape(key)}"]`);
    foldPlace.current = row ? { key, top: row.getBoundingClientRect().top } : null;
    setFolds([key], !shownFolded.has(key));
  };
  useLayoutEffect(() => {
    const place = foldPlace.current;
    if (!place) return;
    foldPlace.current = null;
    const keep = () => {
      const row = document.querySelector<HTMLElement>(`[data-testid="item-row"][data-key="${CSS.escape(place.key)}"]`);
      const list = row?.closest<HTMLElement>(".split-side") ?? row?.closest<HTMLElement>(".app-main");
      if (row && list) list.scrollTop += row.getBoundingClientRect().top - place.top;
    };
    keep();
    const frame = requestAnimationFrame(keep);
    return () => cancelAnimationFrame(frame);
  }, [shownFolded]);

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
  /** Searches in `mode` from now on, and goes back to the search field. */
  const setSearchMode = useCallback(
    (mode: SearchMode) => {
      void update({ items: { search: mode } });
      search.current?.focus();
    },
    [update],
  );
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
    // ⌥T switches the search between the titles and the full text, in the
    // search field too
    if (event.altKey && !event.metaKey && !event.ctrlKey && !event.shiftKey && event.code === "KeyT" && !modalOpen()) {
      event.preventDefault();
      setSearchMode(searchMode === "title" ? "text" : "title");
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
      showRepo={found ? true : layout !== "repository" && (layout === "list" || mixed.has(group))}
      found={found?.get(item.key) ?? null}
      next={prefs.items.show_next && !!nextKeys?.has(item.key)}
      countNested={prefs.items.count_nested}
      actions={rowActions}
    />
  );

  // the page's filters, in the Filter menu and as chips while they are on
  const clearFilters = () =>
    updateParams(
      (next) => {
        for (const key of ["q", "status", "repo", "project", "by", "waiting", "closed", "page"]) next.delete(key);
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
    {
      key: "closed",
      label: "Closed sub-items",
      on: closedSubs,
      onChange: (on) => setParam("closed", on ? "1" : null),
      // with the statuses it adds to, in a section of its own
      under: "status",
      section: "Also show",
    },
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
            <span className="search-field has-mode">
              <Tooltip label={`${SEARCH_MODES[searchMode].tip} (⌥T)`} side="bottom">
                <button
                  type="button"
                  className="search-mode"
                  aria-label={`Search in: ${SEARCH_MODES[searchMode].label}`}
                  aria-haspopup="menu"
                  aria-expanded="false"
                  data-testid="search-mode"
                  onClick={(event) => setModeMenu(event.currentTarget)}
                >
                  <ModeIcon size={13} aria-hidden="true" />
                  <ChevronDown size={11} aria-hidden="true" />
                </button>
              </Tooltip>
              <SearchInput
                ref={search}
                placeholder={searchMode === "title" ? "Search titles…" : "Search work items…"}
                label="Search work items"
                value={q}
                onChange={(text) => {
                  setQ(text);
                  setParam("q", text);
                }}
              />
            </span>
            {modeMenu ? (
              <PopoverMenu
                anchor={modeMenu}
                label="Search in"
                entries={(Object.keys(SEARCH_MODES) as SearchMode[]).map((mode) => ({
                  label: SEARCH_MODES[mode].label,
                  hint: SEARCH_MODES[mode].hint,
                  icon: SEARCH_MODES[mode].icon,
                  checked: mode === searchMode,
                  onPick: () => {
                    void update({ items: { search: mode } });
                    search.current?.focus();
                  },
                }))}
                onClose={() => setModeMenu(null)}
              />
            ) : null}
            {parents.length ? (
              <Tooltip label={allFolded ? "Unfold all" : "Fold all"} side="bottom">
                <button
                  type="button"
                  className="bar-button item-menu-button"
                  aria-label={allFolded ? "Unfold all" : "Fold all"}
                  onClick={foldAll}
                >
                  {allFolded ? <ChevronsUpDown size={15} /> : <ChevronsDownUp size={15} />}
                </button>
              </Tooltip>
            ) : null}
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
              displayToggles={[
                {
                  key: "show_next",
                  label: "Show next item",
                  on: prefs.items.show_next,
                  onChange: (on) => void update({ items: { show_next: on } }),
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
          // the filter's repository, or the open item's, or the focused row's
          defaultRepo={
            params.get("repo") ??
            (openKey ? items?.find((i) => i.key === openKey)?.repo : undefined) ??
            focusedItem?.repo ??
            null
          }
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
            {pageGroups.map(([key, shown]) =>
              found ? (
                <div key={key || "found"} className="inbox-list">
                  {key === CLOSED_GROUP ? (
                    <h2 className="found-closed" data-testid="found-closed">
                      Closed <span>{groupTotals.get(key) ?? shown.length}</span>
                    </h2>
                  ) : null}
                  {shown.map((s) => row(s, ++index, key))}
                </div>
              ) : layout === "list" ? (
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
                    <GroupHeading
                      groupKey={key}
                      project={shown[0]?.item.project}
                      count={groupTotals.get(key) ?? shown.length}
                    />
                  </summary>
                  {shown.map((s) => row(s, ++index, key))}
                </details>
              ),
            )}
          </div>
        )}
        {items ? (
          <Pager
            label="Work items"
            page={page}
            size={size}
            total={topLevel}
            onPage={setPage}
            onSize={(per) =>
              updateParams((next) => {
                if (per === ITEMS_PER_PAGE) next.delete("per");
                else next.set("per", String(per));
                next.delete("page");
              })
            }
          />
        ) : null}
        <footer className="inbox-footer">
          <KeyHints
            hints={[
              { keys: ["J / K"], label: "Move" },
              { keys: ["Enter"], label: "Open" },
              { keys: ["C"], label: "Add" },
              { keys: ["/"], label: "Search" },
              { keys: ["⌥T"], label: "Search mode" },
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
