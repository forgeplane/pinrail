// The side panel a list opens an item in, beside the list: a review or a
// session's thread, and an edge that resizes the panel by dragging or with
// the arrow keys. The lists share one width, stored on this machine.

import { useEffect, useLayoutEffect, useState, type ReactNode, type RefObject } from "react";
import { useResizableWidth } from "../lib/useResizableWidth";

/** The side panel's width, stored on this machine. */
const PANEL_WIDTH = "pinrail.inbox.panel";
/** The narrowest the side panel gets. */
const PANEL_MIN = 420;
/** The narrowest the list beside the side panel gets, unless a page asks for more. */
const LIST_MIN = 320;
/** The room the edge between them takes: 5px, less its negative margins. */
const EDGE = 1;

export function SidePanel({
  split,
  label = "Review",
  listMin = LIST_MIN,
  children,
}: {
  /** the element that holds the list and the panel, which bounds the width */
  split: RefObject<HTMLDivElement | null>;
  /** what the panel shows, which names it: Review or Session */
  label?: string;
  /** the narrowest the list beside the panel gets */
  listMin?: number;
  children: ReactNode;
}): ReactNode {
  const { width, onKeyDown, edgeProps } = useResizableWidth({
    key: PANEL_WIDTH,
    min: PANEL_MIN,
    // the list beside it keeps its room
    max: () => (split.current?.clientWidth ?? window.innerWidth) - listMin - EDGE,
    fallback: null,
    grows: "left",
    anchor: () => split.current?.getBoundingClientRect().right ?? window.innerWidth,
  });
  // What opened the panel, such as a row, and where it was on the screen,
  // read as the panel first renders, before the page changes. Without a
  // panel the page scrolls; with one the list scrolls on its own, from the
  // top, so the list is scrolled to keep the row where it was.
  const [opened] = useState(() => {
    const opener = document.activeElement;
    return opener instanceof HTMLElement && split.current?.contains(opener)
      ? { opener, top: opener.getBoundingClientRect().top }
      : null;
  });
  useLayoutEffect(() => {
    if (!opened?.opener.isConnected) return;
    const list = opened.opener.closest<HTMLElement>(".split-side");
    if (list) list.scrollTop += opened.opener.getBoundingClientRect().top - opened.top;
  }, [opened]);
  // As the panel closes, the open item's row stays where it was too. React
  // removes the panel before it updates the list, so the row's place is
  // read here as the person sees it; once the page has dropped the panel's
  // layout and scrolls as a whole again, it is scrolled to keep the row
  // there.
  useLayoutEffect(
    () => () => {
      const row = split.current?.querySelector<HTMLElement>(".split-side :is(.item-row, .review-row).is-open");
      if (!row) return;
      const top = row.getBoundingClientRect().top;
      const key = row.dataset.key;
      queueMicrotask(() => {
        // a row drawn anew is found by its key
        const now = row.isConnected
          ? row
          : key
            ? document.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`)
            : null;
        const page = now?.closest<HTMLElement>(".app-main");
        if (now && page) page.scrollTop += now.getBoundingClientRect().top - top;
      });
    },
    [split],
  );
  // the focus goes back to what opened the panel, such as a row, when it
  // closes
  useEffect(() => {
    const opener = document.activeElement;
    return () => {
      if (opener instanceof HTMLElement && opener !== document.body && opener.isConnected)
        opener.focus({ preventScroll: true });
    };
  }, []);
  // the stored width, within the room there is now
  const clampWidth = (w: number) => {
    const room = split.current?.clientWidth ?? window.innerWidth;
    return Math.round(Math.max(PANEL_MIN, Math.min(w, room - listMin - EDGE)));
  };
  // the arrow keys start from the width the panel has, stored or not
  const resizeBy = (event: React.KeyboardEvent<HTMLDivElement>) =>
    onKeyDown(event, width ?? split.current?.querySelector<HTMLElement>(".review-panel")?.offsetWidth ?? PANEL_MIN);

  return (
    <>
      <div
        className="panel-resizer"
        {...edgeProps}
        aria-label={`Resize the ${label.toLowerCase()} panel`}
        onKeyDown={resizeBy}
      />
      <aside
        className="review-panel"
        aria-label={label}
        data-testid={`${label.toLowerCase()}-panel`}
        // the list keeps its width, unless that would make the panel
        // narrower than its own minimum
        style={{
          ...(width ? { width: clampWidth(width) } : {}),
          maxWidth: `max(${PANEL_MIN}px, calc(100% - ${listMin + EDGE}px))`,
        }}
      >
        {children}
      </aside>
    </>
  );
}
