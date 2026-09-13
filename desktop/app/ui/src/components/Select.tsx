// A select of the shell's own: a button that shows the choice, a list that
// opens under it. Arrow keys move, Enter and Space choose, Escape and a
// click outside close, typing jumps to a matching option. The list is
// rendered at the end of the body and kept inside the window.

import { Check, ChevronDown } from "lucide-react";
import { useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

export type SelectOption = { value: string; label: string; hint?: string; icon?: ReactNode };

type Props = {
  value: string;
  options: SelectOption[];
  onChange: (value: string) => void;
  /** the accessible name, and the title of the trigger when no option matches */
  label: string;
  icon?: ReactNode;
  id?: string;
};

const GAP = 4;
const MARGIN = 8;

export function Select({ value, options, onChange, label, icon, id }: Props) {
  const listId = useId();
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [pos, setPos] = useState<{ top: number; left: number; minWidth: number } | null>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const typed = useRef({ text: "", at: 0 });
  const current = options.find((o) => o.value === value);

  const openList = () => {
    setActive(Math.max(0, options.findIndex((o) => o.value === value)));
    setOpen(true);
  };
  const close = () => {
    setOpen(false);
    trigger.current?.focus();
  };
  const choose = (option: SelectOption) => {
    onChange(option.value);
    close();
  };

  useLayoutEffect(() => {
    if (!open || !trigger.current || !list.current) return;
    const t = trigger.current.getBoundingClientRect();
    const box = list.current.getBoundingClientRect();
    let left = Math.min(t.left, window.innerWidth - box.width - MARGIN);
    left = Math.max(MARGIN, left);
    let top = t.bottom + GAP;
    if (top + box.height > window.innerHeight - MARGIN) top = Math.max(MARGIN, t.top - box.height - GAP);
    setPos({ top, left, minWidth: t.width });
  }, [open, options.length]);

  useEffect(() => {
    if (!open) return;
    const onPointer = (event: MouseEvent) => {
      const target = event.target as Node;
      if (!list.current?.contains(target) && !trigger.current?.contains(target)) setOpen(false);
    };
    const onScroll = (event: Event) => {
      if (!list.current?.contains(event.target as Node)) setOpen(false);
    };
    const onResize = () => setOpen(false);
    window.addEventListener("mousedown", onPointer);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("mousedown", onPointer);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onResize);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    list.current?.querySelectorAll<HTMLElement>("[role=option]")[active]?.scrollIntoView({ block: "nearest" });
  }, [open, active]);

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (!open) {
      if (["ArrowDown", "ArrowUp", "Enter", " "].includes(event.key)) {
        event.preventDefault();
        event.stopPropagation();
        openList();
      }
      return;
    }
    // the list has the keyboard; the page's own shortcuts stay out of it
    event.stopPropagation();
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        setActive((i) => Math.min(i + 1, options.length - 1));
        break;
      case "ArrowUp":
        event.preventDefault();
        setActive((i) => Math.max(i - 1, 0));
        break;
      case "Home":
        event.preventDefault();
        setActive(0);
        break;
      case "End":
        event.preventDefault();
        setActive(options.length - 1);
        break;
      case "Enter":
      case " ":
        event.preventDefault();
        if (options[active]) choose(options[active]);
        break;
      case "Escape":
        event.preventDefault();
        close();
        break;
      case "Tab":
        setOpen(false);
        break;
      default:
        if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey) {
          const now = Date.now();
          typed.current = { text: (now - typed.current.at < 700 ? typed.current.text : "") + event.key.toLowerCase(), at: now };
          const hit = options.findIndex((o) => o.label.toLowerCase().startsWith(typed.current.text));
          if (hit >= 0) setActive(hit);
        }
    }
  };

  return (
    <>
      <button
        ref={trigger}
        id={id}
        type="button"
        className={`select-trigger ${open ? "is-open" : ""} ${current && current.value ? "has-value" : ""}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        aria-label={label}
        onClick={() => (open ? setOpen(false) : openList())}
        onKeyDown={onKeyDown}
      >
        {current?.icon ?? icon ? <span className="select-icon">{current?.icon ?? icon}</span> : null}
        <span className="select-value">{current?.label ?? label}</span>
        <ChevronDown size={14} className="select-chevron" aria-hidden="true" />
      </button>
      {open
        ? createPortal(
            <ul
              ref={list}
              id={listId}
              role="listbox"
              aria-label={label}
              aria-activedescendant={`${listId}-${active}`}
              className={`select-list ${pos ? "is-placed" : ""}`}
              style={pos ? { top: pos.top, left: pos.left, minWidth: pos.minWidth } : { top: -9999, left: -9999 }}
            >
              {options.map((option, i) => (
                <li
                  key={option.value}
                  id={`${listId}-${i}`}
                  role="option"
                  aria-selected={option.value === value}
                  className={`select-option ${i === active ? "is-active" : ""}`}
                  onMouseEnter={() => setActive(i)}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => choose(option)}
                >
                  <span className="select-option-check">{option.value === value ? <Check size={14} /> : null}</span>
                  {option.icon ? <span className="select-option-icon">{option.icon}</span> : null}
                  <span className="select-option-label">{option.label}</span>
                  {option.hint ? <span className="select-option-hint">{option.hint}</span> : null}
                </li>
              ))}
            </ul>,
            document.body,
          )
        : null}
    </>
  );
}
