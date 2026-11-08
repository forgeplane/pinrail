// Find in page for a part of the app, such as a session's thread: the same
// search the SDK runs in a plugin's view. The text of `root` is searched,
// ignoring case, as one string per block, so a match may cross inline
// elements such as a link. Text with no box does not count. The matches
// are painted with the CSS Custom Highlight API, as `::highlight(pinrail-find)`
// and `::highlight(pinrail-find-current)`, where the webview has it.

/** The most matches counted; more show as "1000+". */
export const FIND_LIMIT = 1000;
const SKIP = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEXTAREA", "OPTION", "INPUT"]);

/** How many matches there are, and which is current, from 1, or 0 for none. */
export type Found = { count: number; current: number };

export type Finder = {
  /** Searches for `text`: the next match when it is the last search, the
   *  previous one with `backwards`, else the first from the top of what
   *  shows. An empty text clears the search. */
  find: (text: string, backwards?: boolean) => Found;
  /** Clears the highlights and stops watching the content. */
  stop: () => void;
};

const supported = () => typeof CSS !== "undefined" && "highlights" in CSS && typeof Highlight === "function";

/** A finder over `root`'s text. `onChange` gets the new count when the
 *  content changes while a search is shown. */
export function finder(root: HTMLElement, onChange: (found: Found) => void): Finder {
  let query = "";
  let matches: Range[] = [];
  let current = -1;
  let observer: MutationObserver | null = null;
  let timer = 0;
  // registered once and changed in place: WebKit repaints a highlight whose
  // ranges change, but not one replaced in the registry
  const all = supported() ? new Highlight() : null;
  const now = supported() ? new Highlight() : null;
  if (all && now) {
    now.priority = 1;
    CSS.highlights.set("pinrail-find", all);
    CSS.highlights.set("pinrail-find-current", now);
  }

  const result = (): Found => ({ count: matches.length, current: current + 1 });

  function search(): Range[] {
    const found: Range[] = [];
    if (!query) return found;
    const pattern = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "giu");
    const display = new Map<Element, string>();
    const displayOf = (el: Element) => {
      let value = display.get(el);
      if (value === undefined) display.set(el, (value = getComputedStyle(el).display));
      return value;
    };
    // the nearest ancestor that is not inline: text inside one is one string
    const blockOf = (el: Element) => {
      for (let node: Element | null = el; node && node !== root; node = node.parentElement) {
        const value = displayOf(node);
        if (value !== "inline" && value !== "contents") return node;
      }
      return root;
    };
    let block: Element | null = null;
    let text = "";
    let pieces: { node: Text; start: number }[] = [];
    const flush = () => {
      pattern.lastIndex = 0;
      let at = 0;
      let hit: RegExpExecArray | null;
      while (found.length < FIND_LIMIT && (hit = pattern.exec(text))) {
        if (!hit[0]) break;
        const start = hit.index;
        const end = start + hit[0].length;
        while (at + 1 < pieces.length && pieces[at + 1].start <= start) at += 1;
        let last = at;
        while (last + 1 < pieces.length && pieces[last + 1].start < end) last += 1;
        const range = document.createRange();
        range.setStart(pieces[at].node, start - pieces[at].start);
        range.setEnd(pieces[last].node, end - pieces[last].start);
        if (range.getClientRects().length > 0) found.push(range);
      }
      text = "";
      pieces = [];
    };
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode() as Text | null; node; node = walker.nextNode() as Text | null) {
      const parent = node.parentElement;
      if (!parent || SKIP.has(parent.tagName)) continue;
      const here = blockOf(parent);
      // white space alone is the space between two inline elements of the
      // block being read, and only layout between blocks
      if (!node.data.trim() && here !== block) continue;
      if (here !== block) {
        flush();
        block = here;
      }
      pieces.push({ node, start: text.length });
      text += node.data;
    }
    flush();
    return found;
  }

  function paint() {
    all?.clear();
    now?.clear();
    for (const range of matches) all?.add(range);
    if (current >= 0) now?.add(matches[current]);
  }

  // the current match, scrolled to the middle of the thread unless it shows
  function reveal(range: Range) {
    const rect = range.getBoundingClientRect();
    let visible = rect.top >= 0 && rect.bottom <= window.innerHeight;
    for (let el = range.startContainer.parentElement; visible && el && el !== document.body; el = el.parentElement) {
      const style = getComputedStyle(el);
      if (style.overflowY !== "visible") {
        const box = el.getBoundingClientRect();
        visible = rect.top >= box.top && rect.bottom <= box.bottom;
      }
    }
    if (!visible) range.startContainer.parentElement?.scrollIntoView({ block: "center", inline: "nearest" });
  }

  // the first match from the top of what shows, or the last before it
  // going backwards
  function nearest(backwards: boolean) {
    if (!matches.length) return -1;
    const top = root.getBoundingClientRect().top;
    const below = matches.findIndex((range) => range.getBoundingClientRect().bottom > top);
    if (below === -1) return backwards ? matches.length - 1 : 0;
    return backwards ? (below === 0 ? matches.length - 1 : below - 1) : below;
  }

  // the content changed while a search shows: the matches are found again
  // without scrolling
  function watch() {
    if (observer) return;
    observer = new MutationObserver(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        if (!query) return;
        matches = search();
        current = matches.length ? Math.min(Math.max(current, 0), matches.length - 1) : -1;
        paint();
        onChange(result());
      }, 150);
    });
    observer.observe(root, { childList: true, subtree: true, characterData: true });
  }

  function unwatch() {
    observer?.disconnect();
    observer = null;
    window.clearTimeout(timer);
  }

  return {
    find(text, backwards = false) {
      if (!text) {
        query = "";
        matches = [];
        current = -1;
        unwatch();
        paint();
        return result();
      }
      const again = text === query && matches.length > 0;
      query = text;
      matches = search();
      if (!matches.length) current = -1;
      else if (again) current = (Math.max(current, 0) + (backwards ? matches.length - 1 : 1)) % matches.length;
      else current = nearest(backwards);
      paint();
      if (current >= 0) reveal(matches[current]);
      watch();
      return result();
    },
    stop() {
      query = "";
      matches = [];
      current = -1;
      unwatch();
      paint();
    },
  };
}
