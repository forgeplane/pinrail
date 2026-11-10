// A list of work items under a text field, opened by typing `#` at the start
// of a word: the text after it narrows the list, arrow keys move through it,
// Enter or Tab puts the item's key in place of what was typed, and Esc
// closes it. The app's Markdown shows a known key as a link to its item.

import { Fragment, useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";
import { createPortal } from "react-dom";
import { api } from "../api/client";
import type { WorkItem } from "../api/types";
import { insertKey, offers, pickerQuery, type Offer } from "../lib/itemPicker";
import { ItemStatusIcon } from "./ItemStatusIcon";

/** The text with the letters at `hits` marked. */
function Marked({ text, hits }: { text: string; hits: number[] }) {
  if (!hits.length) return <>{text}</>;
  const marked = new Set(hits);
  const parts: { text: string; hit: boolean }[] = [];
  for (let i = 0; i < text.length; i++) {
    const hit = marked.has(i);
    const last = parts[parts.length - 1];
    if (last && last.hit === hit) last.text += text[i];
    else parts.push({ text: text[i], hit });
  }
  return (
    <>
      {parts.map((part, i) => (
        <Fragment key={i}>{part.hit ? <mark>{part.text}</mark> : part.text}</Fragment>
      ))}
    </>
  );
}

type Place = { left: number; width: number; top?: number; bottom?: number };
type Open = { at: number; query: string; place: Place };

/** Under the field, or above it when there is no room below. */
function placeBy(el: HTMLElement): Place {
  const rect = el.getBoundingClientRect();
  const below = window.innerHeight - rect.bottom;
  const width = Math.min(Math.max(rect.width, 320), 480);
  const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));
  return below >= 220 || below >= rect.top
    ? { left, width, top: rect.bottom + 4 }
    : { left, width, bottom: window.innerHeight - rect.top + 4 };
}

/** The picker for the text field `box`, whose text is `value`: render what
 *  it returns beside the field. `repo` is the repository whose items come
 *  first. */
export function useItemPicker({
  box,
  value,
  onChange,
  repo,
}: {
  box: RefObject<HTMLTextAreaElement | null>;
  value: string;
  onChange: (text: string) => void;
  repo: string | null;
}) {
  const [open, setOpen] = useState<Open | null>(null);
  const [active, setActive] = useState(0);
  const [items, setItems] = useState<WorkItem[] | null>(null);
  // the # that Esc closed the list for, which does not open it again
  const dismissed = useRef<number | null>(null);
  // where the cursor goes once the inserted key is in the text
  const caret = useRef<number | null>(null);

  // the open items, read when the list first opens
  const asked = useRef(false);
  useEffect(() => {
    if (!open || asked.current) return;
    asked.current = true;
    api
      .items({ status: "open,in_progress,on_hold" })
      .then((d) => setItems(d.items))
      .catch(() => setItems([]));
  }, [open]);

  const shown: Offer[] = open && items ? offers(items, open.query, repo) : [];

  // what the text before the cursor asks for, after each change or move
  const look = () => {
    const el = box.current;
    if (!el || el.selectionStart !== el.selectionEnd) return setOpen(null);
    const found = pickerQuery(el.value, el.selectionStart);
    if (!found || found.at === dismissed.current) {
      if (!found) dismissed.current = null;
      return setOpen(null);
    }
    if (open?.at !== found.at || open.query !== found.query) setActive(0);
    setOpen({ ...found, place: open?.at === found.at ? open.place : placeBy(el) });
  };

  const choose = (offer: Offer) => {
    const el = box.current;
    if (!el || !open) return;
    const next = insertKey(value, open.at, el.selectionStart, offer.item.key);
    caret.current = next.cursor;
    onChange(next.text);
    setOpen(null);
  };

  // before the field's own keys, which save, cancel or leave on them
  const onKey = (event: KeyboardEvent) => {
    if (!open || event.isComposing) return;
    const take = () => {
      event.preventDefault();
      event.stopImmediatePropagation();
    };
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      take();
      const by = event.key === "ArrowDown" ? 1 : -1;
      if (shown.length) setActive((active + by + shown.length) % shown.length);
    } else if ((event.key === "Enter" || event.key === "Tab") && !event.metaKey && !event.ctrlKey && shown[active]) {
      take();
      choose(shown[active]);
    } else if (event.key === "Escape") {
      take();
      dismissed.current = open.at;
      setOpen(null);
    }
  };

  // the listeners call the handlers of the latest render
  const handlers = useRef({ look, onKey });
  useLayoutEffect(() => {
    handlers.current = { look, onKey };
  });
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const onLook = () => handlers.current.look();
    // after React has taken the new text: a render before then would put
    // the old text back in the field
    const onInput = () => requestAnimationFrame(onLook);
    const onKeyDown = (event: KeyboardEvent) => handlers.current.onKey(event);
    const close = () => setOpen(null);
    el.addEventListener("keydown", onKeyDown);
    el.addEventListener("input", onInput);
    el.addEventListener("click", onLook);
    el.addEventListener("keyup", onLook);
    el.addEventListener("blur", close);
    return () => {
      el.removeEventListener("keydown", onKeyDown);
      el.removeEventListener("input", onInput);
      el.removeEventListener("click", onLook);
      el.removeEventListener("keyup", onLook);
      el.removeEventListener("blur", close);
    };
  }, [box]);

  // the cursor after the inserted key, once the text has it
  useLayoutEffect(() => {
    const el = box.current;
    if (caret.current === null || !el) return;
    el.setSelectionRange(caret.current, caret.current);
    caret.current = null;
  }, [value, box]);

  // the active row stays in view
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    list.current?.querySelector(".item-picker-row.is-active")?.scrollIntoView({ block: "nearest" });
  }, [active, open]);

  if (!open) return null;
  return createPortal(
    <div className="item-picker" style={open.place} role="listbox" aria-label="Work items" ref={list}>
      {!items ? (
        <div className="item-picker-empty">Loading…</div>
      ) : shown.length === 0 ? (
        <div className="item-picker-empty">No open work item matches</div>
      ) : (
        shown.map((offer, i) => (
          <div
            key={offer.item.key}
            role="option"
            aria-selected={i === active}
            className={`item-picker-row ${i === active ? "is-active" : ""}`}
            // keeps the focus in the field
            onMouseDown={(event) => event.preventDefault()}
            onMouseEnter={() => setActive(i)}
            onClick={() => choose(offer)}
          >
            <ItemStatusIcon status={offer.item.status} size={13} decorative />
            <span className="item-picker-key">
              <Marked text={offer.item.key} hits={offer.keyHits} />
            </span>
            <span className="item-picker-title truncate">
              <Marked text={offer.item.title} hits={offer.titleHits} />
            </span>
            {offer.item.repo !== repo ? <span className="item-picker-repo truncate">{offer.item.repo}</span> : null}
          </div>
        ))
      )}
    </div>,
    document.body,
  );
}
