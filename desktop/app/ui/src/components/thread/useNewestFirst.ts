// A long thread draws its newest entries first and the rest in the
// background, so that its end shows at once. While the older entries are
// added above, the thread stays at its end, or, when the person has scrolled
// up, keeps what they see in place.

import { startTransition, useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";

/** How many of the newest entries are drawn at once. */
const FIRST = 60;

export function useNewestFirst<T>(
  entries: T[],
  timeline: RefObject<HTMLDivElement | null>,
  {
    all,
    thread,
  }: {
    /** whether to draw every entry at once, to scroll to one of them */
    all: boolean;
    /** the thread shown, which starts again with its newest entries */
    thread: string | null;
  },
) {
  const [wholeFor, setWholeFor] = useState<string | null>(null);
  const whole = all || entries.length <= FIRST || wholeFor === thread;
  const shown = whole ? entries : entries.slice(-FIRST);
  useEffect(() => {
    if (!whole) startTransition(() => setWholeFor(thread));
  }, [whole, thread]);

  // the oldest entry drawn, where it is on screen, and whether the thread
  // is at its end, kept up to date as the person scrolls
  const kept = useRef<{ anchor: Element | null; top: number; atEnd: boolean }>({ anchor: null, top: 0, atEnd: true });
  useLayoutEffect(() => {
    const el = timeline.current;
    if (!el || whole) return;
    const keep = () => {
      const anchor = el.firstElementChild;
      kept.current = {
        anchor,
        top: anchor ? anchor.getBoundingClientRect().top - el.getBoundingClientRect().top : 0,
        atEnd: el.scrollHeight - el.clientHeight - el.scrollTop < 40,
      };
    };
    keep();
    el.addEventListener("scroll", keep, { passive: true });
    return () => el.removeEventListener("scroll", keep);
  }, [timeline, whole, shown]);

  // once the rest is drawn above, before it is painted
  const wasWhole = useRef(whole);
  useLayoutEffect(() => {
    const el = timeline.current;
    const added = whole && !wasWhole.current;
    wasWhole.current = whole;
    if (!el || !added) return;
    const { anchor, top, atEnd } = kept.current;
    if (atEnd) el.scrollTop = el.scrollHeight;
    else if (anchor?.isConnected)
      el.scrollTop += anchor.getBoundingClientRect().top - el.getBoundingClientRect().top - top;
  }, [timeline, whole]);

  return { shown, whole };
}
