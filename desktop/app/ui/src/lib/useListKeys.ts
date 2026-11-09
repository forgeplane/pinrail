// The keys of a list with a side panel, the same on every page: J and K
// move the highlight and the focus, wrapping around at the ends, and Enter
// opens the row lit; with an item open beside the list, J and K open the
// next or the previous one instead. A page keeps its own keys,
// such as D or E, in a handler of its own.

import { useEffect, useRef, useState } from "react";
import { hasMod, isTyping, modalOpen } from "./keys";
import { useWindowKey } from "./useWindowKey";

type Options<T> = {
  items: T[];
  keyOf: (item: T) => string;
  /** the item open in the side panel, by its key */
  openKey: string | null;
  /** opens the row lit, for Enter */
  onOpen: (item: T) => void;
  /** opens another item in the side panel, for J and K */
  onWalk: (item: T) => void;
  /** a row's element, by its place in the list */
  rowOf: (index: number) => HTMLElement | null;
  /** what the panel walks, when more than the rows shown, such as every
   *  page of the list */
  walkItems?: T[];
  /** whether ⌥↓ and ⌥↑ walk the panel too: only where the list is the
   *  reviews that wait, which those keys go to everywhere else */
  altWalk?: boolean;
};

export function useListKeys<T>({
  items,
  keyOf,
  openKey,
  onOpen,
  onWalk,
  rowOf,
  walkItems,
  altWalk = false,
}: Options<T>) {
  const [wanted, setFocused] = useState(0);
  // the highlight stays within the list as it shrinks
  const focused = Math.min(wanted, Math.max(0, items.length - 1));

  // While the keyboard moves the focus the list scrolls under a still
  // pointer, and the row that slides under it must not take the focus
  // back; the next real movement of the pointer hands it over again.
  const keyboard = useRef(false);
  useEffect(() => {
    const onMove = () => {
      keyboard.current = false;
    };
    window.addEventListener("mousemove", onMove);
    return () => window.removeEventListener("mousemove", onMove);
  }, []);
  const moveTo = (index: number) => {
    keyboard.current = true;
    setFocused(index);
  };
  /** the pointer over a row lights it, unless the keyboard moved last */
  const hover = (index: number) => {
    if (!keyboard.current) setFocused(index);
  };

  // The highlight follows the item open in the panel, however it opened,
  // once the list that holds it has loaded; once per item, so that a live
  // update does not take the highlight from the row under the pointer.
  const at = openKey ? items.findIndex((item) => keyOf(item) === openKey) : -1;
  const followed = useRef<string | null>(null);
  useEffect(() => {
    if (!openKey) followed.current = null;
    if (!openKey || at < 0 || followed.current === openKey) return;
    followed.current = openKey;
    setFocused(at);
  }, [openKey, at]);

  const step = (from: number, down: boolean, length = items.length) => (from + (down ? 1 : length - 1)) % length;

  // With an item open, J and K open the next and the previous, from the
  // list or from the view, which forwards them. The capture phase runs
  // this before the page's and the layout's own keys.
  useWindowKey(
    (event) => {
      const walked = walkItems ?? items;
      if (modalOpen() || hasMod(event) || event.shiftKey || !walked.length) return;
      const walk = altWalk && event.altKey && (event.key === "ArrowDown" || event.key === "ArrowUp");
      const jk = !event.altKey && (event.key === "j" || event.key === "k");
      if (!walk && !jk) return;
      if (isTyping(event.target)) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      // the list scrolls to the item the keys open, under a still pointer
      keyboard.current = true;
      const from = walked.findIndex((item) => keyOf(item) === openKey);
      const next = from < 0 ? 0 : step(from, event.key === "j" || event.key === "ArrowDown", walked.length);
      if (keyOf(walked[next]) !== openKey) onWalk(walked[next]);
    },
    { capture: true, enabled: !!openKey },
  );

  // without one, J and K move the highlight and Enter opens the row lit
  useWindowKey(
    (event) => {
      if (modalOpen() || event.metaKey || event.ctrlKey || event.altKey || isTyping(event.target)) return;
      if (!items.length) return;
      if (event.key === "j" || event.key === "k") moveTo(step(focused, event.key === "j"));
      else if (event.key === "Enter" && items[focused]) onOpen(items[focused]);
    },
    { enabled: !openKey },
  );

  // the row lit is scrolled to, and the focus moves with it after a key,
  // so that a screen reader says which row it is on
  useEffect(() => {
    const row = rowOf(focused);
    row?.scrollIntoView({ block: "nearest" });
    if (keyboard.current) row?.focus({ preventScroll: true });
  }, [focused]); // eslint-disable-line react-hooks/exhaustive-deps -- when the highlight moves

  return { focused, setFocused, moveTo, hover };
}
