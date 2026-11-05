// A menu that opens under a button, or above it near the window's bottom: a
// list of actions, with headings and separators. It is drawn at the end of the body, so a scrolling list
// cannot clip it. A click outside, Escape or a pick closes it.

import { Check, type LucideIcon } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { viewportRect } from "../lib/viewport";

export type MenuEntry =
  | {
      kind?: "item";
      label: string;
      icon?: LucideIcon;
      onPick: () => void;
      /** why the action does not apply now, when it does not */
      disabled?: string;
      danger?: boolean;
      checked?: boolean;
    }
  | { kind: "heading"; label: string }
  | { kind: "separator" };

export function PopoverMenu({
  anchor,
  label,
  entries,
  onClose,
  align = "left",
}: {
  anchor: HTMLElement;
  label: string;
  entries: MenuEntry[];
  onClose: () => void;
  /** which edge of the button the menu lines up with */
  align?: "left" | "right";
}) {
  const menu = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  useLayoutEffect(() => {
    const box = viewportRect(anchor);
    const width = menu.current?.offsetWidth ?? 210;
    const height = menu.current?.offsetHeight ?? 0;
    const left = align === "right" ? box.right - width : box.left;
    // under the button, or above it when the window ends first
    const below = box.bottom + 4;
    const top = below + height > window.innerHeight - 4 ? Math.max(4, box.top - height - 4) : below;
    setPos({ top, left: Math.max(4, Math.min(left, window.innerWidth - width - 4)) });
  }, [anchor, align]);

  // the button says its menu is open, which keeps it and its row shown
  useEffect(() => {
    anchor.setAttribute("aria-expanded", "true");
    return () => anchor.setAttribute("aria-expanded", "false");
  }, [anchor]);

  useEffect(() => {
    menu.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    const onDown = (event: MouseEvent) => {
      const target = event.target as Node;
      if (!menu.current?.contains(target) && !anchor.contains(target)) onClose();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onClose();
        anchor.focus();
        return;
      }
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      event.preventDefault();
      const items = [...(menu.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
      const at = items.indexOf(document.activeElement as HTMLButtonElement);
      const next = event.key === "ArrowDown" ? (at + 1) % items.length : (at - 1 + items.length) % items.length;
      items[next]?.focus();
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [anchor, onClose]);

  return createPortal(
    <div
      ref={menu}
      className="menu-popup"
      role="menu"
      aria-label={label}
      style={pos ? { top: pos.top, left: pos.left } : { visibility: "hidden" }}
    >
      {entries.map((entry, i) => {
        if (entry.kind === "separator") return <hr key={i} className="menu-separator" />;
        if (entry.kind === "heading")
          return (
            <div key={i} className="menu-heading">
              {entry.label}
            </div>
          );
        const Icon = entry.icon;
        return (
          <button
            key={i}
            type="button"
            role="menuitem"
            className={`menu-item ${entry.danger ? "is-danger" : ""}`}
            disabled={!!entry.disabled}
            title={entry.disabled}
            onClick={() => {
              onClose();
              entry.onPick();
            }}
          >
            {Icon ? <Icon size={14} className="menu-icon" aria-hidden /> : null}
            <span className="menu-label">{entry.label}</span>
            {entry.checked ? <Check size={13} className="menu-check" /> : null}
          </button>
        );
      })}
    </div>,
    document.body,
  );
}
