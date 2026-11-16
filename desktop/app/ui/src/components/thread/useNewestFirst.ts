// A long thread draws its newest entries, and older ones only as the person
// scrolls up to them, a chunk at a time, so that opening it, switching to it
// and its reloads draw what is near the end, not everything. While older
// entries are added above, the thread stays at its end, or, when the person
// has scrolled up, keeps what they see in place. Find draws the rest a
// chunk a frame, and a jump to an entry further up all of them at once.

import { flushSync } from "react-dom";
import { startTransition, useCallback, useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";

/** How many of the newest entries are drawn first. */
const FIRST = 60;
/** How many older entries are drawn at a time. */
const CHUNK = 100;
/** How many are drawn a frame while find draws the rest. */
const GROW = 50;
/** How close to the top, in heights of the view, older entries are drawn. */
const NEAR_TOP = 2;

export function useNewestFirst<T>(
  entries: T[],
  timelineRef: RefObject<HTMLDivElement | null>,
  {
    all,
    grow = false,
    thread,
    pinned,
  }: {
    /** whether to draw every entry at once, to scroll to one of them */
    all: boolean;
    /** whether to draw the rest a chunk a frame without waiting for the
     *  person to scroll, as find does to search them all */
    grow?: boolean;
    /** the thread shown, which starts again with its newest entries */
    thread: string | null;
    /** whether the thread stays at its end, as `useThreadScroll` keeps it */
    pinned: RefObject<boolean>;
  },
) {
  // how many of the newest entries the thread shown draws
  const [drawn, setDrawn] = useState<{ thread: string | null; count: number }>({ thread, count: FIRST });
  const count = drawn.thread === thread ? drawn.count : FIRST;
  const whole = all || count >= entries.length;
  const shown = whole ? entries : entries.slice(-count);

  /** Draws every entry now, before a jump to one of them. */
  const drawAll = useCallback(() => {
    flushSync(() => setDrawn({ thread, count: Infinity }));
  }, [thread]);

  // the rest, a chunk a frame, each drawn when the page is free
  useEffect(() => {
    if (!grow || whole) return;
    const frame = requestAnimationFrame(() => startTransition(() => setDrawn({ thread, count: count + GROW })));
    return () => cancelAnimationFrame(frame);
  }, [grow, whole, thread, count]);

  // the oldest entry drawn and where it was on screen
  const kept = useRef<{ anchor: Element | null; top: number }>({ anchor: null, top: 0 });

  // once older entries are drawn above, before they are painted, and
  // before the oldest entry is read again below
  const wasShown = useRef(shown.length);
  useLayoutEffect(() => {
    const el = timelineRef.current;
    const added = shown.length > wasShown.current;
    wasShown.current = shown.length;
    if (!el || !added) return;
    const { anchor, top } = kept.current;
    if (pinned.current) el.scrollTop = el.scrollHeight;
    else if (anchor?.isConnected)
      el.scrollTop += anchor.getBoundingClientRect().top - el.getBoundingClientRect().top - top;
  }, [timelineRef, shown.length, pinned]);

  // the oldest entry drawn and where it is on screen, kept up to date as
  // the person scrolls; and more drawn as they near the top
  useLayoutEffect(() => {
    const el = timelineRef.current;
    if (!el || whole) return;
    const onScroll = () => {
      const anchor = el.firstElementChild;
      kept.current = {
        anchor,
        top: anchor ? anchor.getBoundingClientRect().top - el.getBoundingClientRect().top : 0,
      };
      if (el.clientHeight > 0 && el.scrollTop < el.clientHeight * NEAR_TOP)
        startTransition(() => setDrawn({ thread, count: count + CHUNK }));
    };
    onScroll();
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, [timelineRef, whole, thread, count]);

  return { shown, whole, drawAll };
}
