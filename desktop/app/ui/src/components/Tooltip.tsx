// A tooltip for icon buttons and controls: the label, and the shortcut when
// one exists. Appears after a short hover, at once on keyboard focus, and
// goes away on Escape, scroll or a click. Rendered at the end of the body so
// no container can clip it.

import {
  cloneElement,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";

const HOVER_DELAY_MS = 350;
const GAP = 6;
const MARGIN = 8;

type Props = {
  label: ReactNode;
  /** the shortcut, one entry per key: ["⌘", "B"] */
  keys?: string[];
  side?: "top" | "bottom";
  /** show on hover only, not on focus (text fields) */
  hoverOnly?: boolean;
  children: ReactElement<Record<string, unknown>>;
};

export function Tooltip({ label, keys, side = "bottom", hoverOnly = false, children }: Props) {
  const id = useId();
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  const trigger = useRef<HTMLElement | null>(null);
  const tip = useRef<HTMLDivElement | null>(null);
  const timer = useRef<number | undefined>(undefined);

  const show = useCallback((delay: number) => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setOpen(true), delay);
  }, []);
  const hide = useCallback(() => {
    window.clearTimeout(timer.current);
    setOpen(false);
  }, []);

  useLayoutEffect(() => {
    if (!open || !trigger.current || !tip.current) return;
    const t = trigger.current.getBoundingClientRect();
    const box = tip.current.getBoundingClientRect();
    let left = t.left + t.width / 2 - box.width / 2;
    left = Math.max(MARGIN, Math.min(left, window.innerWidth - box.width - MARGIN));
    let top = side === "bottom" ? t.bottom + GAP : t.top - box.height - GAP;
    if (top < MARGIN) top = t.bottom + GAP;
    if (top + box.height > window.innerHeight - MARGIN) top = t.top - box.height - GAP;
    setPos({ top, left });
  }, [open, side]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && hide();
    window.addEventListener("keydown", onKey);
    window.addEventListener("scroll", hide, true);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", hide, true);
    };
  }, [open, hide]);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  const child = children.props;
  const anchored = cloneElement(children, {
    ref: (node: HTMLElement | null) => {
      trigger.current = node;
      const ref = (children as unknown as { ref?: unknown }).ref;
      if (typeof ref === "function") ref(node);
      else if (ref && typeof ref === "object") (ref as { current: HTMLElement | null }).current = node;
    },
    "aria-describedby": open ? id : undefined,
    onMouseEnter: (event: MouseEvent) => {
      (child.onMouseEnter as ((e: MouseEvent) => void) | undefined)?.(event);
      show(HOVER_DELAY_MS);
    },
    onMouseLeave: (event: MouseEvent) => {
      (child.onMouseLeave as ((e: MouseEvent) => void) | undefined)?.(event);
      hide();
    },
    onFocus: (event: FocusEvent) => {
      (child.onFocus as ((e: FocusEvent) => void) | undefined)?.(event);
      if (!hoverOnly) show(0);
    },
    onBlur: (event: FocusEvent) => {
      (child.onBlur as ((e: FocusEvent) => void) | undefined)?.(event);
      hide();
    },
    onMouseDown: (event: MouseEvent) => {
      (child.onMouseDown as ((e: MouseEvent) => void) | undefined)?.(event);
      hide();
    },
  });

  return (
    <>
      {anchored}
      {open
        ? createPortal(
            <div
              ref={tip}
              id={id}
              role="tooltip"
              className={`tooltip ${pos ? "is-placed" : ""}`}
              style={pos ? { top: pos.top, left: pos.left } : { top: -9999, left: -9999 }}
            >
              <span className="tooltip-label">{label}</span>
              {keys && keys.length > 0 ? (
                <span className="tooltip-keys">
                  {keys.map((key, i) => (
                    <kbd key={i}>{key}</kbd>
                  ))}
                </span>
              ) : null}
            </div>,
            document.body,
          )
        : null}
    </>
  );
}
