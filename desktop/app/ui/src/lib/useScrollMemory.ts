import { useEffect, useLayoutEffect, useRef, type RefObject } from "react";
import { useLocation } from "react-router";
import { rememberedScroll, rememberScroll } from "./pageMemory";

/** How long a page's list may take to load before its scroll position is
 *  given up on, in milliseconds. */
const RESTORE_MS = 3000;

/** The element a page's list scrolls in: the list beside an open side
 *  panel, or the whole page. */
function scroller(main: HTMLElement): HTMLElement {
  return main.querySelector<HTMLElement>(":scope > .has-panel > .split-side") ?? main;
}

/**
 * Keeps how far each main page's list is scrolled, and scrolls it back there
 * when the person comes back to the page from another one. The list may
 * load after the page, so the position is set again on each frame until the
 * list is long enough, the person scrolls themselves, or the time runs out.
 */
export function useScrollMemory(main: RefObject<HTMLElement | null>) {
  const { pathname, search } = useLocation();
  const at = useRef({ pathname, search });
  const last = useRef<string | null>(null);

  useEffect(() => {
    const el = main.current;
    if (!el) return;
    const onScroll = (event: Event) => {
      const target = event.target;
      if (target instanceof HTMLElement && target === scroller(el))
        rememberScroll(at.current.pathname, at.current.search, target.scrollTop);
    };
    el.addEventListener("scroll", onScroll, { capture: true, passive: true });
    return () => el.removeEventListener("scroll", onScroll, { capture: true });
  }, [main]);

  useLayoutEffect(() => {
    at.current = { pathname, search };
    const came = last.current !== pathname;
    last.current = pathname;
    const el = main.current;
    const top = came ? rememberedScroll(pathname, search) : undefined;
    if (!el || top === undefined) return;

    const until = performance.now() + RESTORE_MS;
    let frame = 0;
    const stop = () => {
      cancelAnimationFrame(frame);
      for (const type of ["wheel", "touchmove", "keydown", "pointerdown"]) el.removeEventListener(type, stop);
    };
    const step = () => {
      const list = scroller(el);
      list.scrollTop = top;
      if (Math.abs(list.scrollTop - top) < 1 || performance.now() > until) stop();
      else frame = requestAnimationFrame(step);
    };
    for (const type of ["wheel", "touchmove", "keydown", "pointerdown"]) el.addEventListener(type, stop);
    step();
    return stop;
  }, [main, pathname, search]);
}
