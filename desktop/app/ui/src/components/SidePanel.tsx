// The side panel a list opens an item in, beside the list: a review or a
// session's thread, and an edge that resizes the panel by dragging or with
// the arrow keys. The lists share one width, stored on this machine.

import { useState, type ReactNode, type RefObject } from "react";

/** The side panel's width, stored on this machine. */
const PANEL_WIDTH = "pinrail.inbox.panel";
/** The narrowest the side panel gets. */
const PANEL_MIN = 420;
/** The narrowest the list beside the side panel gets. */
const LIST_MIN = 320;

function savedWidth(): number | null {
  try {
    const n = Number(localStorage.getItem(PANEL_WIDTH));
    return n >= PANEL_MIN ? n : null;
  } catch {
    return null;
  }
}

export function SidePanel({
  split,
  label = "Review",
  children,
}: {
  /** the element that holds the list and the panel, which bounds the width */
  split: RefObject<HTMLDivElement | null>;
  /** what the panel shows, which names it: Review or Session */
  label?: string;
  children: ReactNode;
}): ReactNode {
  const [width, setWidth] = useState<number | null>(savedWidth);
  const clampWidth = (w: number) => {
    const room = split.current?.clientWidth ?? window.innerWidth;
    return Math.round(Math.max(PANEL_MIN, Math.min(w, room - LIST_MIN)));
  };
  const keepWidth = (w: number) => {
    setWidth(w);
    try {
      localStorage.setItem(PANEL_WIDTH, String(w));
    } catch {
      // without storage, the width is kept until the app restarts
    }
  };
  const startResize = (event: React.PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const right = split.current?.getBoundingClientRect().right ?? window.innerWidth;
    // stop the view's frame from capturing pointer events during the drag
    document.body.classList.add("is-resizing");
    const onMove = (e: PointerEvent) => setWidth(clampWidth(right - e.clientX));
    const onUp = (e: PointerEvent) => {
      document.body.classList.remove("is-resizing");
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      keepWidth(clampWidth(right - e.clientX));
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };
  const resizeBy = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? 80 : 20;
    const now = width ?? split.current?.querySelector<HTMLElement>(".review-panel")?.offsetWidth ?? PANEL_MIN;
    if (event.key === "ArrowLeft") keepWidth(clampWidth(now + step));
    else if (event.key === "ArrowRight") keepWidth(clampWidth(now - step));
    else return;
    event.preventDefault();
  };

  return (
    <>
      <div
        className="panel-resizer"
        role="separator"
        aria-orientation="vertical"
        aria-label={`Resize the ${label.toLowerCase()} panel`}
        tabIndex={0}
        onPointerDown={startResize}
        onKeyDown={resizeBy}
      />
      <aside
        className="review-panel"
        aria-label={label}
        data-testid={`${label.toLowerCase()}-panel`}
        style={width ? { width: clampWidth(width) } : undefined}
      >
        {children}
      </aside>
    </>
  );
}
