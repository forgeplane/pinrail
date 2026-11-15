// Find in a libghostty terminal, over the screen and the scrollback. Plain
// text uses libghostty's search, which ignores ASCII case. Case-sensitive
// and regular-expression search run in JavaScript over the text of the
// screen's rows. Either way the next match becomes the selection, scrolled
// into view, and the matches in view are highlighted.

import type { SearchOptions, SearchResult } from "../../view";
import type { GhosttyCore } from "./core";
import type { LinkSpan } from "./links";

export type FindTarget = Pick<
  GhosttyCore,
  "clearSearch" | "screenLines" | "scrollToScreenRow" | "scrollbar" | "search" | "searchMatchesInView" | "setSelection"
>;

/** A match on one screen row, in columns, `end` exclusive. */
export interface Match {
  line: number;
  start: number;
  end: number;
}

/** East Asian wide and full-width characters, and emoji shown as emoji,
 *  which take two columns. */
const WIDE =
  /\p{Emoji_Presentation}|\p{Extended_Pictographic}️|[ᄀ-ᅟ⺀-〾ぁ-㏿㐀-䶿一-鿿ꀀ-꓏가-힣豈-﫿︰-﹏＀-｠￠-￦\u{20000}-\u{3FFFD}]/u;

const segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });

/** The column each UTF-16 index of a row's text starts at, with one more
 *  entry for the end of the text. */
export function columnsOf(line: string): number[] {
  const columns = new Array<number>(line.length + 1);
  let column = 0;
  for (const { segment, index } of segmenter.segment(line)) {
    for (let i = 0; i < segment.length; i++) columns[index + i] = column;
    column += WIDE.test(segment) ? 2 : 1;
  }
  columns[line.length] = column;
  return columns;
}

/** The matches of a query on each row, top to bottom; none for an empty
 *  query or a regular expression that does not compile. Matches do not
 *  span rows. */
export function findMatches(lines: readonly string[], query: string, options: SearchOptions): Match[] {
  if (!query) return [];
  let pattern: RegExp;
  try {
    const source = options.regex ? query : query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    pattern = new RegExp(source, `gu${options.caseSensitive ? "" : "i"}`);
  } catch {
    return [];
  }
  const out: Match[] = [];
  lines.forEach((line, index) => {
    let columns: number[] | null = null;
    for (const match of line.matchAll(pattern)) {
      if (match[0].length === 0) continue;
      columns ??= columnsOf(line);
      out.push({ line: index, start: columns[match.index], end: columns[match.index + match[0].length] });
    }
  });
  return out;
}

export class Finder {
  /** The JavaScript search's matches and the one selected. */
  private matches: Match[] = [];
  private current: Match | null = null;
  private native = false;

  constructor(private readonly target: FindTarget) {}

  /** Finds the next match, older first, or with `backwards` newer. */
  find(query: string, options: SearchOptions): SearchResult {
    if (!query) {
      this.clear();
      return { found: false, count: 0 };
    }
    if (!options.caseSensitive && !options.regex) {
      this.matches = [];
      this.current = null;
      this.native = true;
      const count = this.target.search(query, options.backwards);
      return { found: count > 0, count };
    }
    if (this.native) this.target.clearSearch();
    this.native = false;
    this.matches = findMatches(this.target.screenLines(), query, options);
    if (this.matches.length === 0) {
      this.current = null;
      return { found: false, count: 0 };
    }
    // step from the match selected last, or from below the newest
    const at = this.current ? this.matches.findIndex((m) => !before(m, this.current!) && !same(m, this.current!)) : -1;
    const was = this.current ? this.matches.findIndex((m) => same(m, this.current!)) : -1;
    const count = this.matches.length;
    let index: number;
    if (was >= 0) index = options.backwards ? (was + 1) % count : (was - 1 + count) % count;
    else if (this.current && at >= 0) index = options.backwards ? at : (at - 1 + count) % count;
    else index = options.backwards ? 0 : count - 1;
    const match = this.matches[index];
    this.current = match;
    this.target.setSelection({ x: match.start, y: match.line }, { x: match.end - 1, y: match.line }, { screen: true });
    this.target.scrollToScreenRow(match.line);
    return { found: true, count };
  }

  /** The cells of the matches in view, to highlight. */
  matchesInView(): LinkSpan[] {
    if (this.native) return this.target.searchMatchesInView();
    if (this.matches.length === 0) return [];
    const { offset, length } = this.target.scrollbar();
    return this.matches
      .filter((m) => m.line >= offset && m.line < offset + length)
      .map((m) => ({ row: m.line - offset, start: m.start, end: m.end }));
  }

  /** Ends the search: nothing is highlighted any more. */
  clear() {
    this.target.clearSearch();
    this.native = false;
    this.matches = [];
    this.current = null;
  }
}

function same(a: Match, b: Match): boolean {
  return a.line === b.line && a.start === b.start;
}

function before(a: Match, b: Match): boolean {
  return a.line < b.line || (a.line === b.line && a.start < b.start);
}
