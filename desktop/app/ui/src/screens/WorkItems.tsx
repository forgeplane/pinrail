// Work items: what the person and agents record and work through, as one
// list grouped like the inbox, with sub-items nested under their parent or
// each as a row of its own, a quick add at the top, and filters.

import {
  ChevronDown,
  ChevronRight,
  Clock,
  FolderGit2,
  ListChecks,
  ListTree,
  List as ListIcon,
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
import { ApiError, api } from "../api/client";
import type { WorkItem } from "../api/types";
import { AgentIcon } from "../components/AgentIcon";
import { EmptyState } from "../components/EmptyState";
import { GroupHeading } from "../components/GroupHeading";
import { ItemStatusIcon } from "../components/ItemStatusIcon";
import { LayoutSwitch } from "../components/LayoutSwitch";
import { ProjectAvatar } from "../components/ProjectAvatar";
import { Select } from "../components/Select";
import { Tooltip } from "../components/Tooltip";
import { age, stamp } from "../lib/format";
import { groupKey, grouped, mixedGroups } from "../lib/grouping";
import { MOD, SHIFT, modalOpen } from "../lib/keys";
import { KeyHints } from "../components/KeyHints";
import { inScope, scopeOf, scopeOptions, setScope, useUrlParams } from "../lib/url";
import { useLive } from "../state/live";
import { useSettings } from "../state/settings";
import { useToast } from "../state/toasts";

/** How long the search waits for typing to pause before it asks the server. */
const SEARCH_DELAY_MS = 250;

/** The choices of the status filter, and what each asks the core for. */
const STATUSES: { value: string; label: string; query?: string }[] = [
  { value: "", label: "Not closed" },
  { value: "ready", label: "Ready", query: "ready" },
  { value: "blocked", label: "Blocked", query: "blocked" },
  { value: "in_progress", label: "In progress", query: "in_progress" },
  { value: "open", label: "Open", query: "open" },
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

/** An item in the order J and K walk, at its depth in the tree. */
type Shown = { item: WorkItem; depth: number; folded: boolean; children: number; parentTitle?: string };

const reason = (error: unknown) => (error instanceof ApiError ? error.message : "Pinrail could not be reached");

export function WorkItems() {
  const live = useLive();
  const toast = useToast();
  const [params, updateParams] = useUrlParams();
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
      .then(
        (l) =>
          !cancelled &&
          setRepos([
            ...new Set([...l.projects.flatMap((p) => p.repos.map((r) => r.repo)), ...l.loose.map((r) => r.repo)]),
          ]),
      )
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
  const groups = useMemo(
    () =>
      layout === "list"
        ? [["", kept] as [string, WorkItem[]]]
        : grouped(kept, (i) => groupKey(layout, i.repo, i.project)),
    [kept, layout],
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

  const open = useCallback(
    (item: WorkItem) =>
      updateParams((next) => {
        next.set("open", item.key);
      }),
    [updateParams],
  );
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

  // J and K move, Enter opens, / searches, C adds; S, D and X change the
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
      } else if (key === "j") setFocused((f) => Math.min(f + 1, rows.length - 1));
      else if (key === "k") setFocused((f) => Math.max(f - 1, 0));
      else if (event.key === "Enter" && at) open(at);
      else if (key === "c" && !event.shiftKey) {
        event.preventDefault();
        setAdding(true);
      } else if (key === "s" && at) void change(at, { claim: at.status === "in_progress" ? "release" : "take" });
      else if (key === "d" && at) void change(at, { status: at.status === "done" ? "open" : "done" });
      else if (key === "x" && at) void change(at, { status: at.status === "cancelled" ? "open" : "cancelled" });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [rows, focused, open, change]);

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
        className={`item-row ${index === focused ? "is-focused" : ""} ${item.key === openKey ? "is-open" : ""} ${closed ? "is-closed" : ""}`}
        style={{ paddingLeft: 10 + depth * 22 }}
        onMouseEnter={() => setFocused(index)}
        onClick={() => open(item)}
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

  let index = -1;
  return (
    <div className="work-items">
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
          <LayoutSwitch
            value={layout}
            onChange={(next) => void update({ items: { layout: next } })}
            label="Work items layout"
          />
          <span className="segmented" role="radiogroup" aria-label="Sub-items">
            {(
              [
                { value: "nested", label: "Nested", Icon: ListTree },
                { value: "flat", label: "Flat", Icon: ListIcon },
              ] as const
            ).map(({ value, label, Icon }) => (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={prefs.items.nesting === value}
                className={`segmented-text ${prefs.items.nesting === value ? "is-on" : ""}`}
                onClick={() => void update({ items: { nesting: value } })}
              >
                <Icon size={13} aria-hidden="true" /> {label}
              </button>
            ))}
          </span>
        </div>
      </header>
      <div className="item-filters">
        <Select
          id="items-status"
          label="Status"
          value={status}
          onChange={(v) => setParam("status", v || null)}
          options={STATUSES.map((s) => ({ value: s.value, label: s.label, icon: statusIcon(s.value) }))}
        />
        <Select
          id="items-scope"
          searchable
          label="Project or repository"
          icon={<FolderGit2 size={14} />}
          value={scope}
          onChange={(v) => updateParams((next) => setScope(next, v), { replace: true })}
          options={scopeOptions(
            live.namedProjects,
            [...new Set((items ?? []).map((i) => i.repo))].sort(),
            false,
            scope,
          ).map((o) => ({
            value: o.value,
            label: o.label,
            icon: o.project ? <ProjectAvatar project={o.project} size={16} /> : <FolderGit2 size={14} />,
          }))}
        />
        <Select
          id="items-by"
          label="Created by"
          value={by}
          onChange={(v) => setParam("by", v || null)}
          options={[
            { value: "", label: "Created by anyone" },
            { value: "person", label: "Created by you" },
            ...agents.map((a) => ({
              value: a,
              label: `Created by ${a}`,
              icon: <AgentIcon requestedBy={a} size={14} />,
            })),
          ]}
        />
        <button
          type="button"
          className={`chrome-button item-waiting-filter ${waiting ? "is-active" : ""}`}
          aria-pressed={waiting}
          onClick={() => setParam("waiting", waiting ? null : "1")}
        >
          <Clock size={13} aria-hidden="true" /> Waiting on you
        </button>
        {filtered ? (
          <button
            type="button"
            className="chrome-button item-clear"
            onClick={() =>
              updateParams(
                (next) => {
                  for (const key of ["q", "status", "repo", "project", "by", "waiting"]) next.delete(key);
                },
                { replace: true },
              )
            }
          >
            Clear filters
          </button>
        ) : null}
      </div>
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
          {shownGroups.map(([key, shown]) =>
            layout === "list" ? (
              <div key="list" className="inbox-list">
                {shown.map((s) => row(s, ++index, key))}
              </div>
            ) : (
              <details key={key || "none"} className="inbox-repo" open data-testid="inbox-group">
                <summary data-testid="inbox-group-heading">
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
            className="chrome-button"
            aria-label="Add it"
            disabled={!title.trim() || !repo || busy}
            onClick={() => void add(false)}
          >
            <Plus size={14} />
          </button>
        </Tooltip>
        <Tooltip label="Cancel" side="bottom">
          <button type="button" className="chrome-button" aria-label="Cancel" onClick={onClose}>
            <X size={14} />
          </button>
        </Tooltip>
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
