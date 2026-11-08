// How a thread's timeline scrolls: to what it was opened at, with room
// under its last message for the chat box over it, whether it shows its
// newest message, and what was clicked kept in place when its width
// changes.

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";
import type { Review, Session } from "../../api/types";

export function useThreadScroll(
  timeline: RefObject<HTMLDivElement | null>,
  footer: HTMLDivElement | null,
  { focus, items, session, stage }: { focus?: Review; items: Review[]; session: Session | null; stage?: number },
) {
  // once the thread is there, the questions opened are scrolled to, or,
  // for the session alone, the stage asked for or its newest message,
  // before the thread is first painted
  const scrolled = useRef(false);
  useLayoutEffect(() => {
    if (scrolled.current) return;
    if (focus) {
      if (items.length < 2) return;
      scrolled.current = true;
      document.getElementById(`set-${focus.id}`)?.scrollIntoView({ block: "start" });
    } else if (session && timeline.current) {
      scrolled.current = true;
      const started = stage ? document.getElementById(`status-${stage}`) : null;
      if (started) started.scrollIntoView({ block: "center" });
      else timeline.current.scrollTop = timeline.current.scrollHeight;
    }
  }, [timeline, items, focus, session, stage]);

  // the timeline keeps room under its last message for the chat box over it
  useEffect(() => {
    const el = timeline.current;
    if (!footer || !el) return;
    const size = () => el.style.setProperty("--footer-height", `${footer.offsetHeight}px`);
    size();
    const observer = new ResizeObserver(size);
    observer.observe(footer);
    return () => observer.disconnect();
  }, [timeline, footer]);

  // whether the timeline shows its newest message, else a button goes there
  const [atEnd, setAtEnd] = useState(true);
  const checkEnd = useCallback(() => {
    const el = timeline.current;
    if (el) setAtEnd(el.scrollHeight - el.clientHeight - el.scrollTop < 40);
  }, [timeline]);
  useEffect(() => {
    const el = timeline.current;
    if (!el) return;
    el.addEventListener("scroll", checkEnd, { passive: true });
    const observer = new ResizeObserver(checkEnd);
    observer.observe(el);
    return () => {
      el.removeEventListener("scroll", checkEnd);
      observer.disconnect();
    };
  }, [timeline, checkEnd]);
  useEffect(checkEnd, [items, checkEnd]);

  // When the timeline's width changes, as the side panel opens, closes or
  // is resized, its text wraps again and its content moves. What was last
  // clicked in it is scrolled back to where it was on screen.
  useEffect(() => {
    const el = timeline.current;
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
  }, [timeline]);

  return { atEnd };
}
