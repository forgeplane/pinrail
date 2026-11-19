// The split view of a session: how much of its height the summary takes,
// above the terminal, kept for every session, and the divider between them
// that a drag or the arrow keys move.

import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { readStored, writeStored } from "../../lib/storage";

const KEY = "pinrail:split-summary";
/** The summary's share of the height: between these, half by default. */
const MIN = 0.2;
const MAX = 0.8;
const FALLBACK = 0.5;
/** How far an arrow key moves the divider. */
const STEP = 0.05;

const clamp = (share: number) => Math.min(MAX, Math.max(MIN, share));

export function useSplit() {
  const [share, setShare] = useState(() => {
    const stored = Number(readStored(KEY));
    return stored > 0 ? clamp(stored) : FALLBACK;
  });
  const keep = (next: number) => {
    const clamped = clamp(next);
    setShare(clamped);
    writeStored(KEY, clamped.toFixed(3));
  };
  // the drag in progress, ended when the component goes
  const ending = useRef<(() => void) | null>(null);
  useEffect(() => () => ending.current?.(), []);

  const onPointerDown = (event: PointerEvent<HTMLElement>) => {
    event.preventDefault();
    const box = event.currentTarget.parentElement?.getBoundingClientRect();
    if (!box) return;
    const at = (e: globalThis.PointerEvent) => clamp((e.clientY - box.top) / box.height);
    // stop the terminal's canvas and a view's frame from taking the pointer
    document.body.classList.add("is-resizing", "is-resizing-rows");
    const onMove = (e: globalThis.PointerEvent) => setShare(at(e));
    const end = () => {
      document.body.classList.remove("is-resizing", "is-resizing-rows");
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
  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    event.preventDefault();
    keep(share + (event.key === "ArrowDown" ? STEP : -STEP));
  };
  return {
    share,
    /** the rows of the split: the summary, the divider, the terminal */
    rows: `minmax(0, ${share}fr) 9px minmax(0, ${1 - share}fr)`,
    divider: {
      role: "separator",
      "aria-orientation": "horizontal" as const,
      "aria-label": "Resize the summary",
      "aria-valuemin": Math.round(MIN * 100),
      "aria-valuemax": Math.round(MAX * 100),
      "aria-valuenow": Math.round(share * 100),
      tabIndex: 0,
      onPointerDown,
      onKeyDown,
    },
  };
}
