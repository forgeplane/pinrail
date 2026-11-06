// The Work items page's Filter and Display buttons, their menus, and the row
// of chips that names each filter that is on, with a button to remove it.

import { FolderGit2, LayoutGrid, List, ListFilter, ListTree, SlidersHorizontal, X } from "lucide-react";
import { useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useOptionsMenu, type MenuOption } from "./OptionsMenu";
import { Tooltip } from "./Tooltip";

/** A choice of a filter: its value, label and picture. */
export type FilterChoice = { value: string; label: string; mark?: ReactNode };

/** A filter of the page: what it is called, its value, its choices, and
 *  the value that means it is off. */
export type Filter = {
  key: string;
  label: string;
  value: string;
  choices: FilterChoice[];
  /** the value that filters nothing */
  off: string;
  searchable?: boolean;
  onChange: (value: string) => void;
};

/** A filter that is on or off, such as Waiting on you. */
export type Toggle = { key: string; label: string; on: boolean; onChange: (on: boolean) => void };

type Layout = "project" | "repository" | "list";
type Nesting = "nested" | "flat";

const LAYOUTS: { value: Layout; label: string; icon: typeof List }[] = [
  { value: "project", label: "Project", icon: LayoutGrid },
  { value: "repository", label: "Repository", icon: FolderGit2 },
  { value: "list", label: "None", icon: List },
];

const filtersOn = (filters: Filter[], toggles: Toggle[]) =>
  filters.filter((f) => f.value !== f.off).length + toggles.filter((t) => t.on).length;

/** The Filter and Display buttons, each opening its menu. */
export function ItemMenuButtons({
  filters,
  toggles,
  onReset,
  layout,
  nesting,
  onLayout,
  onNesting,
}: {
  filters: Filter[];
  toggles: Toggle[];
  onReset: () => void;
  layout: Layout;
  nesting: Nesting;
  onLayout: (layout: Layout) => void;
  onNesting: (nesting: Nesting) => void;
}) {
  const filterButton = useRef<HTMLButtonElement>(null);
  const displayButton = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState<"filter" | "display" | null>(null);
  const on = filtersOn(filters, toggles);
  return (
    <>
      <Tooltip label="Filter" side="bottom">
        <button
          ref={filterButton}
          type="button"
          className={`bar-button item-menu-button ${open === "filter" ? "is-on" : ""} ${on ? "has-value" : ""}`}
          aria-label="Filter"
          aria-haspopup="menu"
          aria-expanded={open === "filter"}
          onClick={() => setOpen((o) => (o === "filter" ? null : "filter"))}
        >
          <ListFilter size={15} />
        </button>
      </Tooltip>
      <Tooltip label="Display options" side="bottom">
        <button
          ref={displayButton}
          type="button"
          className={`bar-button item-menu-button ${open === "display" ? "is-on" : ""}`}
          aria-label="Display options"
          aria-haspopup="menu"
          aria-expanded={open === "display"}
          onClick={() => setOpen((o) => (o === "display" ? null : "display"))}
        >
          <SlidersHorizontal size={15} />
        </button>
      </Tooltip>
      {open === "filter" && filterButton.current ? (
        <FilterMenu
          anchor={filterButton.current}
          filters={filters}
          toggles={toggles}
          onReset={onReset}
          onClose={() => setOpen(null)}
        />
      ) : null}
      {open === "display" && displayButton.current ? (
        <DisplayMenu
          anchor={displayButton.current}
          layout={layout}
          nesting={nesting}
          onLayout={onLayout}
          onNesting={onNesting}
          onClose={() => setOpen(null)}
        />
      ) : null}
    </>
  );
}

function FilterMenu({
  anchor,
  filters,
  toggles,
  onReset,
  onClose,
}: {
  anchor: HTMLElement;
  filters: Filter[];
  toggles: Toggle[];
  onReset: () => void;
  onClose: () => void;
}) {
  const { menu, pos, setOpen, submenu, toggle } = useOptionsMenu(anchor, onClose);
  if (!pos) return null;
  return createPortal(
    <div
      ref={menu}
      className="menu-popup item-menu opens-left"
      role="menu"
      aria-label="Filter work items"
      style={{ top: pos.top, right: window.innerWidth - pos.right }}
      data-testid="items-filter-menu"
    >
      <div className="menu-heading" onMouseEnter={() => setOpen(null)}>
        Filters
        <button type="button" className="menu-reset" disabled={filtersOn(filters, toggles) === 0} onClick={onReset}>
          Reset
        </button>
      </div>
      {filters.map((f) =>
        submenu(
          f.key,
          f.label,
          f.choices.find((c) => c.value === f.value)?.label ?? "",
          f.choices.map<MenuOption>((c) => ({
            value: c.value,
            label: c.label,
            checked: c.value === f.value,
            radio: true,
            mark: c.mark,
            onPick: () => f.onChange(c.value),
          })),
          f.searchable,
        ),
      )}
      {toggles.map((t) => (
        <span key={t.key}>{toggle(t.label, t.on, () => t.onChange(!t.on))}</span>
      ))}
    </div>,
    document.body,
  );
}

function DisplayMenu({
  anchor,
  layout,
  nesting,
  onLayout,
  onNesting,
  onClose,
}: {
  anchor: HTMLElement;
  layout: Layout;
  nesting: Nesting;
  onLayout: (layout: Layout) => void;
  onNesting: (nesting: Nesting) => void;
  onClose: () => void;
}) {
  const { menu, pos, submenu } = useOptionsMenu(anchor, onClose);
  if (!pos) return null;
  return createPortal(
    <div
      ref={menu}
      className="menu-popup item-menu opens-left"
      role="menu"
      aria-label="Display options"
      style={{ top: pos.top, right: window.innerWidth - pos.right }}
      data-testid="items-display-menu"
    >
      {submenu(
        "grouping",
        "Grouping",
        LAYOUTS.find((l) => l.value === layout)?.label ?? "",
        LAYOUTS.map<MenuOption>((l) => ({
          value: l.value,
          label: l.label,
          checked: l.value === layout,
          radio: true,
          icon: l.icon,
          onPick: () => onLayout(l.value),
        })),
      )}
      {submenu(
        "nesting",
        "Sub-items",
        nesting === "nested" ? "Nested" : "Flat",
        (
          [
            { value: "nested", label: "Nested", icon: ListTree },
            { value: "flat", label: "Flat", icon: List },
          ] as const
        ).map<MenuOption>((n) => ({
          value: n.value,
          label: n.label,
          checked: n.value === nesting,
          radio: true,
          icon: n.icon,
          onPick: () => onNesting(n.value),
        })),
      )}
    </div>,
    document.body,
  );
}

/** A chip for each filter that is on, which removes it, and a button that
 *  removes them all; nothing when no filter is on. */
export function FilterChips({
  filters,
  toggles,
  onReset,
}: {
  filters: Filter[];
  toggles: Toggle[];
  onReset: () => void;
}) {
  const chips = [
    ...filters
      .filter((f) => f.value !== f.off)
      .map((f) => {
        const choice = f.choices.find((c) => c.value === f.value);
        return {
          key: f.key,
          label: f.label,
          value: choice?.label ?? f.value,
          mark: choice?.mark,
          remove: () => f.onChange(f.off),
        };
      }),
    ...toggles
      .filter((t) => t.on)
      .map((t) => ({ key: t.key, label: t.label, value: "", mark: undefined, remove: () => t.onChange(false) })),
  ];
  if (!chips.length) return null;
  return (
    <div className="filter-chips" data-testid="filter-chips">
      {chips.map((c) => (
        <span key={c.key} className="filter-chip" data-testid="filter-chip">
          {c.mark ? <span className="filter-chip-mark">{c.mark}</span> : null}
          <span className="filter-chip-label">{c.value ? `${c.label}: ` : c.label}</span>
          {c.value ? <span className="filter-chip-value">{c.value}</span> : null}
          <button
            type="button"
            className="filter-chip-remove"
            aria-label={`Remove the ${c.label} filter`}
            onClick={c.remove}
          >
            <X size={12} />
          </button>
        </span>
      ))}
      {chips.length > 1 ? (
        <button type="button" className="filter-chips-clear" onClick={onReset}>
          Clear filters
        </button>
      ) : null}
    </div>
  );
}
