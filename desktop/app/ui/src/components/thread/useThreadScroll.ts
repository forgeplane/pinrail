// How a thread's timeline scrolls: to what it was opened at, with room
// under its last message for the chat box over it, whether it shows its
// newest message, and what was clicked kept in place when its width
// changes.

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";
import type { Review, Session } from "../../api/types";

export function useThreadScroll(
  timelineRef: RefObject<HTMLDivElement | null>,
  footer: HTMLDivElement | null,
  {
    focus,
    items,
    session,
    stage,
    placed = false,
  }: {
    focus?: Review;
    items: Review[];
    session: Session | null;
    stage?: number;
    /** whether the thread opens where it was left, which `holdAt` restores
     *  once its entries are drawn, rather than at its end */
    placed?: boolean;
  },
) {
  // once the thread is there, the questions opened are scrolled to, or,
  // for the session alone, the stage asked for or its newest message,
  // before the thread is first painted
  const scrolled = useRef(false);
  // whether the newest message is to stay in view as the thread grows
  const pinned = useRef(false);
  // what the thread was opened at, kept in place as entries above it lay
  // out, until the person scrolls; and where that left the timeline
  const opened = useRef<{ target: Element; offset: number; top: number } | null>(null);
  // the watch for the question to be drawn, while it is not yet
  const waiting = useRef<MutationObserver | null>(null);
  useEffect(() => () => waiting.current?.disconnect(), []);
  useLayoutEffect(() => {
    const el = timelineRef.current;
    if (scrolled.current || !el) return;
    const hold = (target: Element) => {
      opened.current = { target, offset: offsetOf(el, target), top: el.scrollTop };
    };
    const openAt = (target: Element, block: ScrollLogicalPosition) => {
      target.scrollIntoView({ block });
      hold(target);
    };
    if (focus) {
      // a question of a session waits for the session's thread
      if (focus.session && focus.requested_by ? !session : items.length === 0) return;
      scrolled.current = true;
      const id = `set-${focus.id}`;
      const set = document.getElementById(id);
      if (set) openAt(set, "start");
      else {
        // the thread draws its entries a frame after the session arrives:
        // the question is scrolled to as soon as it is drawn
        const drawn = new MutationObserver(() => {
          const found = document.getElementById(id);
          if (!found) return;
          drawn.disconnect();
          openAt(found, "start");
        });
        drawn.observe(el, { childList: true, subtree: true });
        waiting.current = drawn;
      }
    } else if (session) {
      // where it was left is restored by holdAt
      if (placed) return;
      scrolled.current = true;
      const started = stage ? document.getElementById(`status-${stage}`) : null;
      if (started) openAt(started, "center");
      else {
        el.scrollTop = el.scrollHeight;
        pinned.current = true;
      }
    }
  }, [timelineRef, items, focus, session, stage, placed]);

  /** Opens the thread with `target` where it was, `offset` below the top
   *  of the view, and keeps it there as entries above it lay out; at its
   *  end without one. */
  const holdAt = useCallback(
    (target: Element | null, offset: number) => {
      const el = timelineRef.current;
      if (!el || scrolled.current) return;
      scrolled.current = true;
      if (target) {
        el.scrollTop += offsetOf(el, target) - offset;
        opened.current = { target, offset, top: el.scrollTop };
      } else {
        el.scrollTop = el.scrollHeight;
        pinned.current = true;
      }
    },
    [timelineRef],
  );

  // What the thread was opened at stays in place while entries above it
  // lay out. While the newest message is in view, it stays there as the
  // thread grows under it: as an image loads, a question lays out, a reply folds,
  // or the room kept for the chat box grows. Scrolling up ends that, and
  // scrolling back to the end starts it again. Growth under the newest
  // message moves the end away without moving the thread up, so it does
  // not end it.
  useEffect(() => {
    const el = timelineRef.current;
    if (!el) return;
    let last = el.scrollTop;
    const onScroll = () => {
      // hidden, as behind the terminal, it has no size to judge by
      if (el.clientHeight === 0) return;
      // a scroll that is not the one keeping what it opened at in place
      if (opened.current && Math.abs(el.scrollTop - opened.current.top) > 1) opened.current = null;
      if (el.scrollHeight - el.clientHeight - el.scrollTop < 40) pinned.current = true;
      else if (el.scrollTop < last) pinned.current = false;
      last = el.scrollTop;
    };
    const keepAtEnd = () => {
      const at = opened.current;
      if (at?.target.isConnected) {
        el.scrollTop += offsetOf(el, at.target) - at.offset;
        at.top = el.scrollTop;
      } else if (pinned.current) el.scrollTop = el.scrollHeight;
    };
    // the timeline and each entry in it, as entries come and go
    const sizes = new ResizeObserver(keepAtEnd);
    sizes.observe(el);
    for (const entry of el.children) sizes.observe(entry);
    const entries = new MutationObserver((records) => {
      for (const record of records) {
        for (const node of record.removedNodes) if (node instanceof Element) sizes.unobserve(node);
        for (const node of record.addedNodes) if (node instanceof Element) sizes.observe(node);
      }
    });
    entries.observe(el, { childList: true });
    el.addEventListener("scroll", onScroll, { passive: true });
    el.addEventListener("load", keepAtEnd, true);
    return () => {
      sizes.disconnect();
      entries.disconnect();
      el.removeEventListener("scroll", onScroll);
      el.removeEventListener("load", keepAtEnd, true);
    };
  }, [timelineRef]);

  // the timeline keeps room under its last message for the chat box over it
  useEffect(() => {
    const el = timelineRef.current;
    if (!footer || !el) return;
    const size = () => {
      el.style.setProperty("--footer-height", `${footer.offsetHeight}px`);
      if (pinned.current) el.scrollTop = el.scrollHeight;
    };
    size();
    const observer = new ResizeObserver(size);
    observer.observe(footer);
    return () => observer.disconnect();
  }, [timelineRef, footer]);

  // whether the timeline shows its newest message, else a button goes there
  const [atEnd, setAtEnd] = useState(true);
  const checkEnd = useCallback(() => {
    const el = timelineRef.current;
    if (el) setAtEnd(el.scrollHeight - el.clientHeight - el.scrollTop < 40);
  }, [timelineRef]);
  useEffect(() => {
    const el = timelineRef.current;
    if (!el) return;
    el.addEventListener("scroll", checkEnd, { passive: true });
    const observer = new ResizeObserver(checkEnd);
    observer.observe(el);
    return () => {
      el.removeEventListener("scroll", checkEnd);
      observer.disconnect();
    };
  }, [timelineRef, checkEnd]);

  useEffect(checkEnd, [items, checkEnd]);

  // When the timeline's width changes, as the side panel opens, closes or
  // is resized, its text wraps again and its content moves. What was last
  // clicked in it, or else the entry at the top of its view, is scrolled
  // back to where it was on screen. At its end, it stays at its end.
  useEffect(() => {
    const el = timelineRef.current;
    if (!el) return;
    let width = el.clientWidth;
    // what was clicked in the timeline, which stays the anchor as the
    // timeline scrolls, or else the entry at the top of its view
    let anchor: { target: Element; top: number; clicked: boolean } | null = null;
    const offset = (target: Element) => target.getBoundingClientRect().top - el.getBoundingClientRect().top;
    const atTop = () => {
      const target = topEntry(el);
      anchor = target ? { target, top: offset(target), clicked: false } : null;
    };
    const onDown = (event: PointerEvent) => {
      // elsewhere, as in the details beside it, the entry at the top
      if (event.target instanceof Element && el.contains(event.target))
        anchor = { target: event.target, top: offset(event.target), clicked: true };
      else atTop();
    };
    // a scroll by the person moves the anchor with it; one caused by the
    // new width, before the observer runs, does not
    const onScroll = () => {
      if (el.clientWidth !== width) return;
      if (anchor?.clicked && anchor.target.isConnected) anchor.top = offset(anchor.target);
      else atTop();
    };
    const observer = new ResizeObserver(() => {
      if (el.clientWidth === width) return;
      width = el.clientWidth;
      if (!pinned.current && anchor?.target.isConnected) el.scrollTop += offset(anchor.target) - anchor.top;
    });
    document.addEventListener("pointerdown", onDown, true);
    el.addEventListener("scroll", onScroll, { passive: true });
    observer.observe(el);
    return () => {
      document.removeEventListener("pointerdown", onDown, true);
      el.removeEventListener("scroll", onScroll);
      observer.disconnect();
    };
  }, [timelineRef]);

  return { atEnd, pinned, holdAt };
}

/** How far below the top of `timeline` the top of `target` is. */
function offsetOf(timeline: Element, target: Element): number {
  return target.getBoundingClientRect().top - timeline.getBoundingClientRect().top;
}

/** The element at the top of `timeline`'s view and how far below the top
 *  it is, or `null` at its end. */
export function placeOf(timeline: Element, atEnd: boolean): { element: Element; offset: number } | null {
  if (atEnd || timeline.clientHeight === 0) return null;
  const top = timeline.getBoundingClientRect().top;
  const element = [...timeline.children].find((kid) => kid.getBoundingClientRect().bottom > top);
  return element ? { element, offset: offsetOf(timeline, element) } : null;
}

/** The entry at the top of `timeline`'s view: the first whose bottom is
 *  below the top, found by halves, as the entries are in order. */
function topEntry(timeline: Element): Element | null {
  const kids = timeline.children;
  const top = timeline.getBoundingClientRect().top;
  let low = 0;
  let high = kids.length - 1;
  if (high < 0) return null;
  while (low < high) {
    const mid = (low + high) >> 1;
    if (kids[mid]!.getBoundingClientRect().bottom > top) high = mid;
    else low = mid + 1;
  }
  return kids[low] ?? null;
}
