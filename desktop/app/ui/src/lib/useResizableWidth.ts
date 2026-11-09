// A panel's width set by dragging the edge beside it or with the arrow keys
// on that edge, stored on this machine: the sidebar, the side panel and a
// session's details. The drag ends cleanly when the panel goes away during
// it.

import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { readStored, writeStored } from "./storage";

type Options = {
  /** the storage key */
  key: string;
  min: number;
  /** the widest it gets, which may depend on the room there is */
  max: number | (() => number);
  /** the width when none is stored; null lets the CSS decide */
  fallback: number | null;
  /** where the panel grows from its edge: "right" for a sidebar on the left
   *  of its edge, "left" for a panel on the right of it */
  grows: "left" | "right";
  /** the x of the panel's fixed side, from the edge's element */
  anchor: (edge: HTMLElement) => number;
  /** during a drag, shows a width without storing it in state, such as on
   *  a CSS variable; by default the width is set as the drag goes */
  preview?: (width: number) => void;
};

export function useResizableWidth({ key, min, max, fallback, grows, anchor, preview }: Options) {
  const clamp = (w: number) => Math.round(Math.max(min, Math.min(w, typeof max === "function" ? max() : max)));
  const [width, setWidth] = useState<number | null>(() => {
    const n = Number(readStored(key));
    return n >= min ? clamp(n) : fallback;
  });
  const keep = (w: number) => {
    setWidth(w);
    writeStored(key, String(w));
  };

  // the drag in progress, ended when the component goes
  const ending = useRef<(() => void) | null>(null);
  useEffect(() => () => ending.current?.(), []);

  const onPointerDown = (event: PointerEvent<HTMLElement>) => {
    event.preventDefault();
    const from = anchor(event.currentTarget);
    const at = (e: globalThis.PointerEvent) => clamp(grows === "right" ? e.clientX - from : from - e.clientX);
    // stop a view's frame from capturing pointer events during the drag
    document.body.classList.add("is-resizing");
    const onMove = (e: globalThis.PointerEvent) => (preview ? preview(at(e)) : setWidth(at(e)));
    const end = () => {
      document.body.classList.remove("is-resizing");
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      ending.current = null;
    };
    const onUp = (e: globalThis.PointerEvent) => {
      end();
      keep(at(e));
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    ending.current = end;
  };

  const onKeyDown = (event: KeyboardEvent<HTMLElement>, current = width ?? min) => {
    const step = event.shiftKey ? 80 : 20;
    const wider = grows === "right" ? "ArrowRight" : "ArrowLeft";
    const narrower = grows === "right" ? "ArrowLeft" : "ArrowRight";
    if (event.key === wider) keep(clamp(current + step));
    else if (event.key === narrower) keep(clamp(current - step));
    else return;
    event.preventDefault();
  };

  const edgeProps = {
    role: "separator" as const,
    "aria-orientation": "vertical" as const,
    "aria-valuenow": width ?? undefined,
    "aria-valuemin": min,
    "aria-valuemax": typeof max === "function" ? undefined : max,
    tabIndex: 0,
    onPointerDown,
  };
  return { width, keep, onKeyDown, edgeProps };
}
