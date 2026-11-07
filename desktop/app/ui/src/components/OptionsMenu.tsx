// The parts of an options menu, as the sidebar's sessions heading and the
// Work items page use them: a popup under its button, closed by a click
// outside it or by Escape, whose items open submenus of radio or checkbox
// options to the right on hover or click, some with a field to search them,
// or toggle a choice. The popup is drawn at the end of the body, so a
// scrolling container cannot clip it.

import { Check, ChevronRight, type LucideIcon } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { viewportRect } from "../lib/viewport";

/** An option of a submenu. */
export type MenuOption = {
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

/** The state and parts of an options menu anchored under `anchor`. */
export function useOptionsMenu(anchor: HTMLElement, onClose: () => void) {
  const menu = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState<string | null>(null);
  // what is typed in a searchable submenu
  const [query, setQuery] = useState("");
  // in a searchable submenu, the match the arrow keys move to and Enter picks
  const [active, setActive] = useState(0);
  useEffect(() => setQuery(""), [open]);
  useEffect(() => setActive(0), [open, query]);
  // under the button: its left edge, or its right edge for a menu that
  // opens to the left
  const [pos, setPos] = useState<{ top: number; left: number; right: number } | null>(null);

  useLayoutEffect(() => {
    const box = viewportRect(anchor);
    setPos({ top: box.bottom + 4, left: box.left, right: box.right });
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

  const submenu = (key: string, text: string, value: string, all: MenuOption[], searchable = false) => {
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
                onKeyDown={(event) => {
                  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                    event.preventDefault();
                    const step = event.key === "ArrowDown" ? 1 : -1;
                    setActive((a) => Math.min(Math.max(a + step, 0), Math.max(options.length - 1, 0)));
                  } else if (event.key === "Enter" && options[active]) {
                    event.preventDefault();
                    options[active].onPick();
                  }
                }}
              />
            ) : null}
            {all.length === 0 ? <div className="menu-note">None yet</div> : null}
            {all.length > 0 && options.length === 0 ? <div className="menu-note">No matches</div> : null}
            {options.map((o, i) => (
              <button
                key={o.value}
                type="button"
                role={o.radio ? "menuitemradio" : "menuitemcheckbox"}
                aria-checked={o.checked}
                className={`menu-item ${searchable && i === active ? "is-active" : ""}`}
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

  return { menu, pos, setOpen, submenu, toggle };
}
