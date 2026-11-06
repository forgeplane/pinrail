// The menu of the sidebar's sessions heading: how the sessions are grouped,
// ordered and shown, and which are listed. Items with a submenu open it to
// the right on hover or click. The menu is drawn at the end of the body, so
// the sidebar's scrolling cannot clip it.

import { Check, ChevronRight, CircleDot, Clock, FolderGit2, LayoutGrid, List, type LucideIcon } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { viewportRect } from "../lib/viewport";
import {
  type Choices,
  type Detail,
  type Grouping,
  type Ordering,
  type State,
  NO_FILTERS,
  filtersSet,
} from "../lib/sidebarSessions";

const GROUPINGS: [Grouping, string][] = [
  ["project", "Project"],
  ["repository", "Repository"],
  ["status", "Status"],
  ["updated", "Updated"],
  ["none", "None"],
];
const ORDERINGS: [Ordering, string][] = [
  ["status", "Status"],
  ["updated", "Updated"],
];
/** The icon of each grouping and ordering, by its value. */
const ICONS: Record<Grouping, LucideIcon> = {
  project: LayoutGrid,
  repository: FolderGit2,
  status: CircleDot,
  updated: Clock,
  none: List,
};
const DETAILS: [Detail, string][] = [
  ["activity", "Last activity"],
  ["status", "Latest status"],
  ["repo", "Repository"],
  ["branch", "Branch"],
  ["agent", "Agent"],
];
const STATES: [State, string][] = [
  ["working", "Working"],
  ["quiet", "Quiet"],
  ["done", "Done"],
];

/** A project or a repository the sessions can be filtered by. */
export type ScopeChoice = { value: string; label: string; icon: ReactNode };

type Option = {
  value: string;
  label: string;
  checked: boolean;
  onPick: () => void;
  radio?: boolean;
  icon?: LucideIcon;
  /** a picture in place of the icon, such as a project's avatar */
  mark?: ReactNode;
  /** why the option does not apply now, when it does not */
  disabled?: string;
};

/** Moves a submenu up, beside its item, as far as it runs past the
 *  window's bottom. */
function fitInWindow(node: HTMLElement | null) {
  if (!node) return;
  node.style.top = "";
  const over = node.getBoundingClientRect().bottom - (window.innerHeight - 8);
  if (over > 0) node.style.top = `${parseFloat(getComputedStyle(node).top) - over}px`;
}

/** A list of strings with one value turned on or off. */
const toggled = <T extends string>(list: T[], value: T) =>
  list.includes(value) ? list.filter((v) => v !== value) : [...list, value];

export function SidebarSessionsMenu({
  anchor,
  choices,
  agents,
  repos,
  collapsedAll,
  onChange,
  onCollapseAll,
  onClose,
}: {
  anchor: HTMLElement;
  choices: Choices;
  /** the agents there are sessions of, and the projects and repositories
   *  as values and labels, for the filters */
  agents: string[];
  repos: ScopeChoice[];
  collapsedAll: boolean;
  onChange: (change: Partial<Choices>) => void;
  onCollapseAll: (collapse: boolean) => void;
  onClose: () => void;
}) {
  const menu = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState<string | null>(null);
  // what is typed in a searchable submenu
  const [query, setQuery] = useState("");
  useEffect(() => setQuery(""), [open]);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  useLayoutEffect(() => {
    const box = viewportRect(anchor);
    setPos({ top: box.bottom + 4, left: box.left });
  }, [anchor]);

  // a click outside, Escape or a scroll closes the menu
  useEffect(() => {
    const onDown = (event: MouseEvent) => {
      const target = event.target as Node;
      if (!menu.current?.contains(target) && !anchor.contains(target)) onClose();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      onClose();
      anchor.focus();
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [anchor, onClose]);

  const radios = <T extends string>(list: [T, string][], value: T, pick: (v: T) => void): Option[] =>
    list.map(([v, label]) => ({
      value: v,
      label,
      checked: v === value,
      onPick: () => pick(v),
      radio: true,
      icon: ICONS[v as Grouping],
    }));
  const checks = <T extends string>(list: [T, string][], values: T[], pick: (v: T) => void): Option[] =>
    list.map(([v, label]) => ({ value: v, label, checked: values.includes(v), onPick: () => pick(v) }));
  const named = (values: string[], all: string) => (values.length ? values.join(", ") : all);
  const label = (list: [string, string][], value: string) => list.find(([v]) => v === value)?.[1] ?? "";

  const submenu = (key: string, text: string, value: string, all: Option[], searchable = false) => {
    const words = query.trim().toLowerCase();
    const options = searchable && words ? all.filter((o) => o.label.toLowerCase().includes(words)) : all;
    return (
      <div className="menu-sub" onMouseEnter={() => setOpen(key)}>
        <button
          type="button"
          role="menuitem"
          aria-haspopup="menu"
          aria-expanded={open === key}
          className={`menu-item ${open === key ? "is-open" : ""}`}
          // the pointer opened it already on its way to the click
          onClick={() => setOpen(key)}
        >
          {text}
          <span className="menu-value">{value}</span>
          <ChevronRight size={13} className="menu-chevron" />
        </button>
        {open === key ? (
          <div
            ref={fitInWindow}
            className={`menu-popup menu-submenu ${searchable ? "is-searchable" : ""}`}
            role="menu"
            aria-label={text}
          >
            {searchable && all.length ? (
              <input
                autoFocus
                className="select-search"
                type="text"
                autoComplete="off"
                autoCorrect="off"
                autoCapitalize="off"
                spellCheck={false}
                placeholder="Type to filter"
                aria-label={`Filter ${text.toLowerCase()}`}
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            ) : null}
            {all.length === 0 ? <div className="menu-note">None yet</div> : null}
            {all.length > 0 && options.length === 0 ? <div className="menu-note">No matches</div> : null}
            {options.map((o) => (
              <button
                key={o.value}
                type="button"
                role={o.radio ? "menuitemradio" : "menuitemcheckbox"}
                aria-checked={o.checked}
                className="menu-item"
                disabled={!!o.disabled}
                title={o.disabled}
                onClick={o.onPick}
              >
                {o.mark ? <span className="menu-icon menu-mark">{o.mark}</span> : null}
                {o.icon ? <o.icon size={14} className="menu-icon" aria-hidden /> : null}
                <span className="menu-label">{o.label}</span>
                {o.checked ? <Check size={13} className="menu-check" /> : null}
              </button>
            ))}
          </div>
        ) : null}
      </div>
    );
  };
  const toggle = (text: string, checked: boolean, onPick: () => void, extra?: ReactNode) => (
    <button
      type="button"
      role="menuitemcheckbox"
      aria-checked={checked}
      className="menu-item"
      onMouseEnter={() => setOpen(null)}
      onClick={onPick}
    >
      {text}
      {extra}
      {checked ? <Check size={13} className="menu-check" /> : null}
    </button>
  );

  if (!pos) return null;
  return createPortal(
    <div
      ref={menu}
      className="menu-popup sidebar-sessions-menu"
      role="menu"
      aria-label="Sessions in the sidebar"
      style={{ top: pos.top, left: pos.left }}
      data-testid="sidebar-sessions-menu"
    >
      {submenu(
        "grouping",
        "Grouping",
        label(GROUPINGS, choices.grouping),
        radios(GROUPINGS, choices.grouping, (grouping) => onChange({ grouping })),
      )}
      {submenu(
        "ordering",
        "Ordering",
        label(ORDERINGS, choices.ordering),
        radios(ORDERINGS, choices.ordering, (ordering) => onChange({ ordering })),
      )}
      {toggle("Show details", choices.show_details, () => onChange({ show_details: !choices.show_details }))}
      {submenu(
        "details",
        "Details",
        "",
        checks(DETAILS, choices.details, (d) => onChange({ details: toggled(choices.details, d) })).map((o) =>
          // the group names the repository already
          o.value === "repo" && choices.grouping === "repository"
            ? { ...o, disabled: "The groups name the repository" }
            : o,
        ),
      )}
      {toggle("Show reviews", choices.show_reviews, () => onChange({ show_reviews: !choices.show_reviews }))}
      <hr className="menu-separator" />
      <div className="menu-heading" onMouseEnter={() => setOpen(null)}>
        Filters
        <button
          type="button"
          className="menu-reset"
          disabled={filtersSet(choices) === 0}
          onClick={() => onChange(NO_FILTERS)}
        >
          Reset
        </button>
      </div>
      {submenu(
        "agents",
        "Agent",
        named(choices.agents, ""),
        checks(
          agents.map((a) => [a, a]),
          choices.agents,
          (a) => onChange({ agents: toggled(choices.agents, a) }),
        ),
      )}
      {submenu(
        "repos",
        "Project or repository",
        choices.repos.length ? String(choices.repos.length) : "",
        repos.map((r) => ({
          value: r.value,
          label: r.label,
          mark: r.icon,
          checked: choices.repos.includes(r.value),
          onPick: () => onChange({ repos: toggled(choices.repos, r.value) }),
        })),
        true,
      )}
      {submenu(
        "states",
        "Status",
        named(
          choices.states.map((s) => label(STATES, s)),
          "All",
        ),
        checks(STATES, choices.states, (s) => onChange({ states: toggled(choices.states, s) })),
      )}
      {toggle("Archived", choices.archived, () => onChange({ archived: !choices.archived }))}
      <hr className="menu-separator" />
      <button
        type="button"
        role="menuitem"
        className="menu-item"
        onMouseEnter={() => setOpen(null)}
        onClick={() => {
          onCollapseAll(!collapsedAll);
          onClose();
        }}
      >
        {collapsedAll ? "Expand all groups" : "Collapse all groups"}
      </button>
    </div>,
    document.body,
  );
}
