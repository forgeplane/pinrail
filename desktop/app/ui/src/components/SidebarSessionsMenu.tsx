// The menu of the sidebar's sessions heading: how the sessions are grouped,
// ordered and shown, and which are listed, from the parts of an options
// menu.

import { Archive, CircleDot, Clock, FolderGit2, LayoutGrid, List, type LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import { AgentIcon } from "./AgentIcon";
import { useOptionsMenu, type MenuOption } from "./OptionsMenu";
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
  const { menu, pos, setOpen, submenu, toggle } = useOptionsMenu(anchor, onClose);
  const radios = <T extends string>(list: [T, string][], value: T, pick: (v: T) => void): MenuOption[] =>
    list.map(([v, label]) => ({
      value: v,
      label,
      checked: v === value,
      onPick: () => pick(v),
      radio: true,
      icon: ICONS[v as Grouping],
    }));
  const checks = <T extends string>(
    list: [T, string][],
    values: T[],
    pick: (v: T) => void,
    mark?: (v: T) => ReactNode,
  ): MenuOption[] =>
    list.map(([v, label]) => ({
      value: v,
      label,
      checked: values.includes(v),
      onPick: () => pick(v),
      mark: mark?.(v),
    }));
  const named = (values: string[], all: string) => (values.length ? values.join(", ") : all);
  const label = (list: [string, string][], value: string) => list.find(([v]) => v === value)?.[1] ?? "";

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
      {toggle("Show waiting reviews", choices.show_reviews, () => onChange({ show_reviews: !choices.show_reviews }))}
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
          (a) => <AgentIcon requestedBy={a} size={14} />,
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
        checks(
          STATES,
          choices.states,
          (s) => onChange({ states: toggled(choices.states, s) }),
          // the mark its sessions show on their agent's icon
          (s) => <span className={`state-mark is-${s}`} aria-hidden />,
        ),
      )}
      {toggle("Include archived", choices.archived, () => onChange({ archived: !choices.archived }), null, Archive)}
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
