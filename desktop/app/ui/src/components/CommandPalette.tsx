// Search across the app: pending reviews, decided ones, work items, the
// stages agents started, plugins and the shell's actions, from one field. Cmd+K opens it; arrows move, Enter opens,
// Tab cycles the filter, Escape closes.

import { GitCommitHorizontal, Search, Zap, type LucideIcon } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useNavigate } from "react-router";
import { api, EVERY_ITEM_STATUS } from "../api/client";
import type { Plugin, Review, Stage, WorkItem } from "../api/types";
import { age, settledAt } from "../lib/format";
import { reviewPath } from "../lib/url";
import { AgentIcon } from "./AgentIcon";
import { itemPath } from "./ItemChip";
import { ItemStatusIcon } from "./ItemStatusIcon";
import { useLive } from "../state/live";
import { useSettings } from "../state/settings";
import { OutcomeBadge } from "./Badges";
import { PluginIcon } from "./PluginIcon";
import { ReviewIcon, reviewLabel } from "./ReviewIcon";
import { sessionPath } from "./Thread";
import { composing } from "../lib/keys";

export type PaletteAction = {
  id: string;
  label: string;
  keys?: string[];
  icon?: LucideIcon;
  run: () => void;
};

type Filter = "all" | "inbox" | "history" | "items" | "stages" | "plugins" | "actions";
const FILTERS: { key: Filter; label: string }[] = [
  { key: "all", label: "All" },
  { key: "inbox", label: "Inbox" },
  { key: "history", label: "History" },
  { key: "items", label: "Work items" },
  { key: "stages", label: "Stages" },
  { key: "plugins", label: "Plugins" },
  { key: "actions", label: "Actions" },
];

type Item = { key: string; group: Filter; title: ReactNode; meta?: ReactNode; icon: ReactNode; run: () => void };

const GROUP_TITLES: Record<Exclude<Filter, "all">, [string, string]> = {
  inbox: ["Pending reviews", "Pending reviews"],
  history: ["Recent decisions", "Decisions"],
  items: ["Work items", "Work items"],
  stages: ["Stages", "Stages"],
  plugins: ["Plugins", "Plugins"],
  actions: ["Actions", "Actions"],
};

const matches = (q: string, ...fields: (string | null | undefined)[]) =>
  !q || fields.some((f) => f && f.toLowerCase().includes(q));

const reviewText = (r: Review) => [r.title, r.plugin, r.requested_by, r.origin.repo, r.origin.workflow, r.origin.ref];

export function CommandPalette({ onClose, actions }: { onClose: () => void; actions: PaletteAction[] }) {
  const { pending: waiting } = useLive();
  const navigate = useNavigate();
  const { inbox, labs } = useSettings().settings;
  const openIn = inbox.open_in;
  // the stages belong to sessions, which are in Settings › Labs
  const filters = labs.sessions ? FILTERS : FILTERS.filter((f) => f.key !== "stages");
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  // the highlighted entry, the first again after each change of the search
  const [active, setActive] = useState(0);
  const search = (text: string) => {
    setQuery(text);
    setActive(0);
  };
  const show = (next: Filter) => {
    setFilter(next);
    setActive(0);
  };
  const [history, setHistory] = useState<Review[]>([]);
  const [plugins, setPlugins] = useState<Plugin[]>([]);
  const [stages, setStages] = useState<Stage[]>([]);
  const [workItems, setWorkItems] = useState<WorkItem[]>([]);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const q = query.trim().toLowerCase();
  const all = filter === "all";

  useEffect(() => {
    api
      .plugins()
      .then(({ plugins }) => setPlugins(plugins))
      .catch(() => setPlugins([]));
    // every work item, found by its key or its title as the person types
    api
      .items({ status: EVERY_ITEM_STATUS })
      .then(({ items }) => setWorkItems(items))
      .catch(() => setWorkItems([]));
    const focus = window.setTimeout(() => input.current?.focus(), 0);
    return () => window.clearTimeout(focus);
  }, []);

  // decided reviews come from the server, a moment after typing stops
  useEffect(() => {
    if (filter !== "all" && filter !== "history") return;
    // an answer for a search typed over, or a palette closed, is dropped
    let current = true;
    const timer = window.setTimeout(
      () => {
        api
          .listReviews({
            status: "decided,discarded,withdrawn,expired",
            q: q || undefined,
            include_revised: "true",
            limit: all ? "5" : "20",
          })
          .then((listing) => current && setHistory(listing.reviews))
          .catch(() => current && setHistory([]));
      },
      q ? 150 : 0,
    );
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [q, filter, all]);

  // the stages agents started, from the server, once something is typed
  useEffect(() => {
    // with nothing typed, the stages show only under their own filter
    if (!labs.sessions || (filter !== "all" && filter !== "stages") || (!q && all)) return;
    let current = true;
    const timer = window.setTimeout(
      () => {
        api
          .stages({ q: q || undefined, limit: all ? "5" : "20" })
          .then((found) => current && setStages(found.stages))
          .catch(() => current && setStages([]));
      },
      q ? 150 : 0,
    );
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [q, filter, all, labs.sessions]);

  const items = useMemo<Item[]>(() => {
    const out: Item[] = [];
    const want = (g: Filter) => all || filter === g;
    if (want("inbox")) {
      const pending = waiting.filter((r) => matches(q, ...reviewText(r)));
      for (const r of all ? pending.slice(0, 5) : pending) {
        out.push({
          key: `inbox:${r.id}`,
          group: "inbox",
          icon: <ReviewIcon review={r} />,
          title: r.title,
          meta: (
            <>
              <span className="mono">{reviewLabel(r)}</span>
              <span>{age(r.created_at)}</span>
            </>
          ),
          run: () => navigate(reviewPath(r.id, openIn)),
        });
      }
    }
    if (want("history")) {
      for (const r of history) {
        out.push({
          key: `history:${r.id}`,
          group: "history",
          icon: <ReviewIcon review={r} />,
          title: r.title,
          meta: (
            <>
              <OutcomeBadge review={r} />
              <span className="mono">{r.plugin}</span>
              <span>{age(settledAt(r))}</span>
            </>
          ),
          run: () => navigate(`/reviews/${r.id}`),
        });
      }
    }
    if (want("items") && (q || filter === "items")) {
      // what is not closed first, then by key
      const found = workItems
        .filter((i) => matches(q, i.key.toLowerCase(), i.title))
        .sort(
          (a, b) =>
            Number(a.status === "done" || a.status === "cancelled") -
              Number(b.status === "done" || b.status === "cancelled") ||
            a.repo.localeCompare(b.repo) ||
            a.number - b.number,
        );
      for (const i of all ? found.slice(0, 5) : found) {
        out.push({
          key: `item:${i.id}`,
          group: "items",
          icon: <ItemStatusIcon status={i.status} />,
          title: i.title,
          meta: (
            <>
              <span className="mono">{i.key}</span>
              <span>{i.repo}</span>
            </>
          ),
          run: () => navigate(itemPath(i.key)),
        });
      }
    }
    if (labs.sessions && want("stages")) {
      for (const s of !q && all ? [] : stages) {
        out.push({
          key: `stage:${s.id}`,
          group: "stages",
          icon: <GitCommitHorizontal size={15} />,
          title: s.text,
          meta: (
            <>
              <span className="with-icon">
                <AgentIcon requestedBy={s.agent} size={12} /> {s.agent}
              </span>
              {s.repo ? <span>{s.repo}</span> : null}
              <span>{age(s.at)}</span>
            </>
          ),
          run: () => navigate(sessionPath({ agent: s.agent, id: s.session }), { state: { stage: s.id } }),
        });
      }
    }
    if (want("plugins")) {
      const found = plugins.filter((p) => matches(q, p.name, p.title));
      for (const p of all ? found.slice(0, q ? 5 : 3) : found) {
        out.push({
          key: `plugin:${p.name}`,
          group: "plugins",
          icon: <PluginIcon icon={p.icon} />,
          title: p.title || p.name,
          meta: (
            <span className="mono">
              {p.name} {p.version}
            </span>
          ),
          run: () => navigate("/", { state: { settings: "plugins", plugin: p.name } }),
        });
      }
    }
    if (want("actions")) {
      const found = actions.filter((a) => matches(q, a.label));
      for (const a of all && !q ? found.slice(0, 4) : found) {
        const Icon = a.icon ?? Zap;
        out.push({
          key: `action:${a.id}`,
          group: "actions",
          icon: <Icon size={15} />,
          title: a.label,
          meta: a.keys ? (
            <span className="palette-keys">
              {a.keys.map((k, i) => (
                <kbd key={i}>{k}</kbd>
              ))}
            </span>
          ) : undefined,
          run: a.run,
        });
      }
    }
    return out;
  }, [all, filter, q, waiting, history, workItems, stages, plugins, actions, navigate, openIn, labs.sessions]);

  useEffect(() => {
    list.current?.querySelectorAll<HTMLElement>("[data-item]")[active]?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const pick = (item: Item) => {
    onClose();
    item.run();
  };
  const cycle = (dir: 1 | -1) => {
    const at = filters.findIndex((f) => f.key === filter);
    show(filters[(at + dir + filters.length) % filters.length].key);
  };
  const onKeyDown = (event: React.KeyboardEvent) => {
    event.stopPropagation();
    if (composing(event)) return;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        setActive((i) => Math.min(i + 1, Math.max(0, items.length - 1)));
        break;
      case "ArrowUp":
        event.preventDefault();
        setActive((i) => Math.max(i - 1, 0));
        break;
      case "Enter":
        event.preventDefault();
        if (items[active]) pick(items[active]);
        break;
      case "Tab":
        event.preventDefault();
        cycle(event.shiftKey ? -1 : 1);
        break;
      case "Escape":
        event.preventDefault();
        onClose();
        break;
    }
  };

  let index = -1;
  let lastGroup: Filter | null = null;

  return (
    <div className="app-dialog-backdrop palette-backdrop" onMouseDown={onClose}>
      <div
        className="palette"
        role="dialog"
        aria-modal="true"
        aria-label="Search"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
      >
        <label className="palette-field">
          <Search size={16} aria-hidden="true" />
          <input
            autoCorrect="off"
            autoCapitalize="off"
            ref={input}
            type="text"
            value={query}
            placeholder="Search reviews, plugins, actions…"
            aria-label="Search"
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => search(e.target.value)}
          />
        </label>
        <div className="palette-filters" role="tablist">
          {filters.map((f) => (
            <button
              key={f.key}
              type="button"
              role="tab"
              aria-selected={filter === f.key}
              className={filter === f.key ? "is-active" : ""}
              onClick={() => show(f.key)}
              tabIndex={-1}
            >
              {f.label}
            </button>
          ))}
        </div>
        <div className="palette-list" ref={list} role="listbox">
          {items.length === 0 ? (
            <p className="palette-empty">{q ? `Nothing matches “${query.trim()}”.` : "Nothing here yet."}</p>
          ) : null}
          {items.map((item) => {
            index += 1;
            const here = index;
            const header =
              item.group !== lastGroup ? GROUP_TITLES[item.group as Exclude<Filter, "all">][all ? 0 : 1] : null;
            lastGroup = item.group;
            return (
              <div key={item.key}>
                {header ? <div className="palette-group">{header}</div> : null}
                <div
                  data-item
                  role="option"
                  aria-selected={here === active}
                  className={`palette-item ${here === active ? "is-active" : ""}`}
                  onMouseEnter={() => setActive(here)}
                  onClick={() => pick(item)}
                >
                  <span className="palette-icon">{item.icon}</span>
                  <span className="palette-title truncate">{item.title}</span>
                  {item.meta ? <span className="palette-meta">{item.meta}</span> : null}
                </div>
              </div>
            );
          })}
        </div>
        <div className="palette-foot">
          <span>
            <kbd>↑</kbd>
            <kbd>↓</kbd> Select
          </span>
          <span>
            <kbd>↵</kbd> Open
          </span>
          <span>
            <kbd>Tab</kbd> Change filter
          </span>
          <span>
            <kbd>Esc</kbd> Close
          </span>
        </div>
      </div>
    </div>
  );
}
