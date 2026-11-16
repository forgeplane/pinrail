// How a thread's timeline scrolls: to what it was opened at, with room
// under its last message for the chat box over it, whether it shows its
// newest message, and what was clicked kept in place when its width
// changes.

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";
import type { Review, Session } from "../../api/types";

export function useThreadScroll(
  timelineRef: RefObject<HTMLDivElement | null>,
  footer: HTMLDivElement | null,
  { focus, items, session, stage }: { focus?: Review; items: Review[]; session: Session | null; stage?: number },
) {
  // once the thread is there, the questions opened are scrolled to, or,
  // for the session alone, the stage asked for or its newest message,
  // before the thread is first painted
  const scrolled = useRef(false);
  // whether the newest message is to stay in view as the thread grows
  const pinned = useRef(false);
  useLayoutEffect(() => {
    if (scrolled.current) return;
    if (focus) {
      if (items.length < 2) return;
      scrolled.current = true;
      document.getElementById(`set-${focus.id}`)?.scrollIntoView({ block: "start" });
    } else if (session && timelineRef.current) {
      scrolled.current = true;
      const started = stage ? document.getElementById(`status-${stage}`) : null;
      if (started) started.scrollIntoView({ block: "center" });
      else {
        timelineRef.current.scrollTop = timelineRef.current.scrollHeight;
        pinned.current = true;
      }
    }
  }, [timelineRef, items, focus, session, stage]);

  // While the newest message is in view, it stays there as the thread
  // grows under it: as an image loads, a question lays out, a reply folds,
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
      if (el.scrollHeight - el.clientHeight - el.scrollTop < 40) pinned.current = true;
      else if (el.scrollTop < last) pinned.current = false;
      last = el.scrollTop;
    };
    const keepAtEnd = () => {
      if (pinned.current) el.scrollTop = el.scrollHeight;
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
  // clicked in it is scrolled back to where it was on screen.
  useEffect(() => {
    const el = timelineRef.current;
    if (!el) return;
    let width = el.clientWidth;
    let anchor: { target: Element; top: number } | null = null;
    const offset = (target: Element) => target.getBoundingClientRect().top - el.getBoundingClientRect().top;
    const onDown = (event: PointerEvent) => {
      anchor = event.target instanceof Element ? { target: event.target, top: offset(event.target) } : null;
    };
    // a scroll by the person moves the anchor with it; one caused by the
    // new width, before the observer runs, does not
    const onScroll = () => {
      if (anchor?.target.isConnected && el.clientWidth === width) anchor.top = offset(anchor.target);
    };
    const observer = new ResizeObserver(() => {
      if (el.clientWidth === width) return;
      width = el.clientWidth;
      if (anchor?.target.isConnected) el.scrollTop += offset(anchor.target) - anchor.top;
    });
    el.addEventListener("pointerdown", onDown);
    el.addEventListener("scroll", onScroll, { passive: true });
    observer.observe(el);
    return () => {
      el.removeEventListener("pointerdown", onDown);
      el.removeEventListener("scroll", onScroll);
      observer.disconnect();
    };
  }, [timelineRef]);

  return { atEnd, pinned };
}
