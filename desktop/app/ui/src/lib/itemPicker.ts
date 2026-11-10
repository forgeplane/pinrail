// The work items a person can name while writing a comment or a
// description: typed `#` opens them, and the text after it narrows them.

import type { WorkItem } from "../api/types";
import { fuzzyMatch } from "./fuzzy";

/** An item offered, with where its key and title matched. */
export type Offer = { item: WorkItem; keyHits: number[]; titleHits: number[] };

/** The most items the picker lists. */
export const OFFERED = 50;

/** The query being typed after a `#` that starts a word, before the
 *  cursor, with where its `#` is; null when the cursor is not in one. */
export function pickerQuery(text: string, cursor: number): { at: number; query: string } | null {
  const before = text.slice(0, cursor);
  const match = before.match(/(?:^|\s)#([^\s#]*)$/);
  if (!match) return null;
  return { at: cursor - match[1].length - 1, query: match[1] };
}

/** The items that match `query`, those of `repo` first, each group by how
 *  well they match, and the newest first among equals. */
export function offers(items: WorkItem[], query: string, repo: string | null): Offer[] {
  const found: (Offer & { score: number })[] = [];
  for (const item of items) {
    const key = fuzzyMatch(query, item.key);
    const title = fuzzyMatch(query, item.title);
    if (!key && !title) continue;
    // the key typed whole, or its number, counts most
    const exact = query && (item.key.toLowerCase() === query.toLowerCase() || String(item.number) === query);
    const score = Math.max(key ? key.score + 30 : -Infinity, title?.score ?? -Infinity) + (exact ? 500 : 0);
    found.push({
      item,
      keyHits: key && (!title || key.score + 30 >= title.score) ? key.indices : [],
      titleHits: title && (!key || title.score > key.score + 30) ? title.indices : [],
      score,
    });
  }
  const mine = (o: Offer) => (o.item.repo === repo ? 0 : 1);
  found.sort((a, b) => mine(a) - mine(b) || b.score - a.score || b.item.updated_at.localeCompare(a.item.updated_at));
  return found.slice(0, OFFERED).map(({ item, keyHits, titleHits }) => ({ item, keyHits, titleHits }));
}

/** The text with the `#query` at `at` replaced by `key`, and where the
 *  cursor goes after it. */
export function insertKey(text: string, at: number, cursor: number, key: string): { text: string; cursor: number } {
  const after = text.slice(cursor);
  const space = after.length && /^\s/.test(after) ? "" : " ";
  const inserted = key + space;
  return { text: text.slice(0, at) + inserted + after, cursor: at + inserted.length };
}
