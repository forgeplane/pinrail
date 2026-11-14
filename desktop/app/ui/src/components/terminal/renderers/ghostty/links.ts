// Links in a libghostty terminal: OSC 8 hyperlinks from the cells, and
// plain http, https and file URLs found in the text of the visible rows.

import type { GhosttyRow } from "./types";

/** Cells of one row, from `start` to `end`, exclusive. */
export interface LinkSpan {
  row: number;
  start: number;
  end: number;
}

export interface Link {
  uri: string;
  spans: LinkSpan[];
}

const URL_PATTERN = /\b(?:https?:\/\/|file:\/\/)[^\s<>"'`]+/g;
const TRAILING = /[.,;:!?'"]$/;
const PAIRS: Record<string, string> = { ")": "(", "]": "[", "}": "{", ">": "<" };

/** A URL without the punctuation that ends the sentence around it. A
 *  closing bracket stays when the URL opened it, as in Wikipedia links. */
export function trimUrl(url: string): string {
  for (;;) {
    const last = url.at(-1) ?? "";
    if (TRAILING.test(last)) {
      url = url.slice(0, -1);
      continue;
    }
    const open = PAIRS[last];
    if (open && count(url, open) < count(url, last)) {
      url = url.slice(0, -1);
      continue;
    }
    return url;
  }
}

function count(text: string, char: string): number {
  let n = 0;
  for (const c of text) if (c === char) n++;
  return n;
}

/**
 * The plain URL under a cell, if any. The row is joined with the rows it
 * wraps into and from, so a URL that wraps is found whole.
 */
export function urlAt(rows: readonly GhosttyRow[], col: number, row: number): Link | null {
  let first = row;
  while (first > 0 && rows[first - 1].wrapsToNext) first--;
  let last = row;
  while (last < rows.length - 1 && rows[last].wrapsToNext) last++;
  // the joined text, and the cell each character came from
  let text = "";
  const cells: { row: number; col: number }[] = [];
  for (let y = first; y <= last; y++) {
    rows[y].cells.forEach((cell, x) => {
      // a wide cluster's spacer has no text of its own
      if (cell.wide === 2) return;
      const chars = cell.text || " ";
      for (let i = 0; i < chars.length; i++) cells.push({ row: y, col: x });
      text += chars;
    });
  }
  for (const match of text.matchAll(URL_PATTERN)) {
    const uri = trimUrl(match[0]);
    const from = match.index;
    const to = from + uri.length;
    const hit = cells.findIndex((c) => c.row === row && c.col === col);
    if (hit < from || hit >= to) continue;
    return { uri, spans: spans(cells.slice(from, to), rows) };
  }
  return null;
}

/** The cells of a run of characters, as one span per row. */
function spans(cells: { row: number; col: number }[], rows: readonly GhosttyRow[]): LinkSpan[] {
  const out: LinkSpan[] = [];
  for (const { row, col } of cells) {
    const wide = rows[row].cells[col + 1]?.wide === 2 ? 2 : 1;
    const span = out.at(-1);
    if (span && span.row === row) span.end = Math.max(span.end, col + wide);
    else out.push({ row, start: col, end: col + wide });
  }
  return out;
}

/**
 * The cells of the OSC 8 link under a cell: the cells around it, along
 * its row and the rows it wraps with, that carry the same URI.
 */
export function hyperlinkAt(
  rows: readonly GhosttyRow[],
  uriAt: (col: number, row: number) => string | null,
  col: number,
  row: number,
): Link | null {
  const uri = uriAt(col, row);
  if (!uri) return null;
  const width = rows[row].cells.length;
  const same = (x: number, y: number) => uriAt(x, y) === uri;
  // walk back to the link's first cell, then forward to its last
  let y = row;
  let x = col;
  for (;;) {
    if (x > 0 && same(x - 1, y)) x--;
    else if (x === 0 && y > 0 && rows[y - 1].wrapsToNext && same(width - 1, y - 1)) {
      y--;
      x = width - 1;
    } else break;
  }
  const out: LinkSpan[] = [{ row: y, start: x, end: x + 1 }];
  for (;;) {
    const span = out[out.length - 1];
    if (span.end < width && same(span.end, span.row)) span.end++;
    else if (span.end === width && rows[span.row].wrapsToNext && span.row + 1 < rows.length && same(0, span.row + 1)) {
      out.push({ row: span.row + 1, start: 0, end: 1 });
    } else break;
  }
  return { uri, spans: out };
}
