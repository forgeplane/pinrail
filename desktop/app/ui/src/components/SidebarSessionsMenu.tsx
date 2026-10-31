// The menu of the sidebar's sessions heading: how the sessions are grouped,
// ordered and shown, and which are listed. Items with a submenu open it to
// the right on hover or click. The menu is drawn at the end of the body, so
// the sidebar's scrolling cannot clip it.

import { Check, ChevronRight, CircleDot, Clock, FolderGit2, List, type LucideIcon } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import {
  type Choices,
  type Detail,
  type Grouping,
  type Ordering,
  type State,
  DEFAULT_CHOICES,
  filtersSet,
} from "../lib/sidebarSessions";

const GROUPINGS: [Grouping, string][] = [
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

type Option = {
  value: string;
  label: string;
  checked: boolean;
  onPick: () => void;
  radio?: boolean;
  icon?: LucideIcon;
  /** why the option does not apply now, when it does not */
  disabled?: string;
};

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
  /** the agents and repositories there are sessions of, for the filters */
  agents: string[];
  repos: string[];
  collapsedAll: boolean;
  onChange: (change: Partial<Choices>) => void;
  onCollapseAll: (collapse: boolean) => void;
  onClose: () => void;
}) {
  const menu = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  useLayoutEffect(() => {
    const box = anchor.getBoundingClientRect();
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

  const submenu = (key: string, text: string, value: string, options: Option[]) => (
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
        <div className="menu-popup menu-submenu" role="menu" aria-label={text}>
          {options.length === 0 ? <div className="menu-note">None yet</div> : null}
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
              {o.icon ? <o.icon size={14} className="menu-icon" aria-hidden /> : null}
              <span className="menu-label">{o.label}</span>
              {o.checked ? <Check size={13} className="menu-check" /> : null}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
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
          onClick={() =>
            onChange({
              agents: DEFAULT_CHOICES.agents,
              repos: DEFAULT_CHOICES.repos,
              states: DEFAULT_CHOICES.states,
              archived: DEFAULT_CHOICES.archived,
            })
          }
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
        "Repository",
        choices.repos.length ? String(choices.repos.length) : "",
        checks(
          repos.map((r) => [r, r === "-" ? "No repository" : r]),
          choices.repos,
          (r) => onChange({ repos: toggled(choices.repos, r) }),
        ),
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
