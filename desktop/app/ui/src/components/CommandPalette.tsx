// Search across the app: pending reviews, decided ones, plugins and the
// shell's actions, from one field. Cmd+K opens it; arrows move, Enter opens,
// Tab cycles the filter, Escape closes.

import { Search, Zap, type LucideIcon } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useNavigate } from "react-router";
import { api } from "../api/client";
import type { Plugin, Review } from "../api/types";
import { age } from "../lib/format";
import { useLive } from "../state/live";
import { OutcomeBadge } from "./Badges";
import { PluginIcon } from "./PluginIcon";

export type PaletteAction = {
  id: string;
  label: string;
  keys?: string[];
  icon?: LucideIcon;
  run: () => void;
};

type Filter = "all" | "inbox" | "history" | "plugins" | "actions";
const FILTERS: { key: Filter; label: string }[] = [
  { key: "all", label: "All" },
  { key: "inbox", label: "Inbox" },
  { key: "history", label: "History" },
  { key: "plugins", label: "Plugins" },
  { key: "actions", label: "Actions" },
];

type Item = { key: string; group: Filter; title: ReactNode; meta?: ReactNode; icon: ReactNode; run: () => void };

const GROUP_TITLES: Record<Exclude<Filter, "all">, [string, string]> = {
  inbox: ["Pending reviews", "Pending reviews"],
  history: ["Recent decisions", "Decisions"],
  plugins: ["Plugins", "Plugins"],
  actions: ["Actions", "Actions"],
};

const matches = (q: string, ...fields: (string | null | undefined)[]) =>
  !q || fields.some((f) => f && f.toLowerCase().includes(q));

const reviewText = (r: Review) => [r.title, r.plugin, r.requested_by, r.origin.repo, r.origin.workflow, r.origin.ref];

export function CommandPalette({ open, onClose, actions }: { open: boolean; onClose: () => void; actions: PaletteAction[] }) {
  const { pending: waiting, pluginIcon } = useLive();
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [active, setActive] = useState(0);
  const [history, setHistory] = useState<Review[]>([]);
  const [plugins, setPlugins] = useState<Plugin[]>([]);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const q = query.trim().toLowerCase();
  const all = filter === "all";

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setFilter("all");
    setActive(0);
    api
      .plugins()
      .then(({ plugins }) => setPlugins(plugins))
      .catch(() => setPlugins([]));
    const focus = window.setTimeout(() => input.current?.focus(), 0);
    return () => window.clearTimeout(focus);
  }, [open]);

  // decided reviews come from the server, a moment after typing stops
  useEffect(() => {
    if (!open || (filter !== "all" && filter !== "history")) return;
    const timer = window.setTimeout(() => {
      api
        .listReviews({ status: "decided,discarded,withdrawn,expired", q: q || undefined, include_revised: "true", limit: all ? "5" : "20" })
        .then((listing) => setHistory(listing.reviews))
        .catch(() => setHistory([]));
    }, q ? 150 : 0);
    return () => window.clearTimeout(timer);
  }, [open, q, filter, all]);

  const items = useMemo<Item[]>(() => {
    const out: Item[] = [];
    const want = (g: Filter) => all || filter === g;
    if (want("inbox")) {
      const pending = waiting.filter((r) => matches(q, ...reviewText(r)));
      for (const r of all ? pending.slice(0, 5) : pending) {
        out.push({
          key: `inbox:${r.id}`,
          group: "inbox",
          icon: <PluginIcon icon={pluginIcon(r.plugin)} />,
          title: r.title,
          meta: (
            <>
              <span className="mono">{r.plugin}</span>
              <span>{age(r.created_at)}</span>
            </>
          ),
          run: () => navigate(`/reviews/${r.id}`),
        });
      }
    }
    if (want("history")) {
      for (const r of history) {
        out.push({
          key: `history:${r.id}`,
          group: "history",
          icon: <PluginIcon icon={pluginIcon(r.plugin)} />,
          title: r.title,
          meta: (
            <>
              <OutcomeBadge review={r} />
              <span className="mono">{r.plugin}</span>
              <span>{age(r.decision?.decided_at ?? r.withdrawn_at ?? r.expires_at)}</span>
            </>
          ),
          run: () => navigate(`/reviews/${r.id}`),
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
              {p.name} v{p.version}
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
  }, [all, filter, q, waiting, pluginIcon, history, plugins, actions, navigate]);

  useEffect(() => setActive(0), [q, filter]);
  useEffect(() => {
    list.current?.querySelectorAll<HTMLElement>("[data-item]")[active]?.scrollIntoView({ block: "nearest" });
  }, [active]);

  if (!open) return null;

  const pick = (item: Item) => {
    onClose();
    item.run();
  };
  const cycle = (dir: 1 | -1) => {
    const at = FILTERS.findIndex((f) => f.key === filter);
    setFilter(FILTERS[(at + dir + FILTERS.length) % FILTERS.length].key);
  };
  const onKeyDown = (event: React.KeyboardEvent) => {
    event.stopPropagation();
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
      <div className="palette" role="dialog" aria-modal="true" aria-label="Search" onMouseDown={(e) => e.stopPropagation()} onKeyDown={onKeyDown}>
        <label className="palette-field">
          <Search size={16} aria-hidden="true" />
          <input
            ref={input}
            type="text"
            value={query}
            placeholder="Search reviews, plugins, actions…"
            aria-label="Search"
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        <div className="palette-filters" role="tablist">
          {FILTERS.map((f) => (
            <button key={f.key} type="button" role="tab" aria-selected={filter === f.key} className={filter === f.key ? "is-active" : ""} onClick={() => setFilter(f.key)} tabIndex={-1}>
              {f.label}
            </button>
          ))}
        </div>
        <div className="palette-list" ref={list} role="listbox">
          {items.length === 0 ? <p className="palette-empty">{q ? `Nothing matches “${query.trim()}”.` : "Nothing here yet."}</p> : null}
          {items.map((item) => {
            index += 1;
            const here = index;
            const header = item.group !== lastGroup ? GROUP_TITLES[item.group as Exclude<Filter, "all">][all ? 0 : 1] : null;
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
                  <span className="palette-title">{item.title}</span>
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
