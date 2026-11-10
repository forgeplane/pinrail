// Fuzzy matching for pickers: the letters of a query in order, not
// necessarily next to each other, as an editor's file finder matches. A
// match that keeps the letters together, or starts them at the start of a
// word, scores higher.

export type Match = { score: number; indices: number[] };

const BOUNDARY = /[\s\-_/.:([]/;

/** Where `query` matches `text`, and how well, or null when it does not.
 *  The case is ignored. */
export function fuzzyMatch(query: string, text: string): Match | null {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (!q) return { score: 0, indices: [] };

  // the query as one piece, at the start of a word where it can be
  let at = -1;
  for (let i = t.indexOf(q); i !== -1; i = t.indexOf(q, i + 1)) {
    if (i === 0 || BOUNDARY.test(t[i - 1])) {
      at = i;
      break;
    }
    if (at === -1) at = i;
  }
  if (at !== -1) {
    const start = at === 0 || BOUNDARY.test(t[at - 1]);
    const score = 100 + q.length * 10 + (start ? 40 : 0) + (at === 0 ? 20 : 0);
    return { score, indices: Array.from({ length: q.length }, (_, k) => at + k) };
  }

  // the letters in order: each at the next start of a word that has it, or
  // else at its next place
  const indices: number[] = [];
  let score = 0;
  let from = 0;
  for (const ch of q) {
    let found = -1;
    for (let i = from; i < t.length; i++) {
      if (t[i] !== ch) continue;
      if (i === 0 || BOUNDARY.test(t[i - 1])) {
        found = i;
        break;
      }
      if (found === -1) found = i;
    }
    if (found === -1) return null;
    const last = indices[indices.length - 1];
    if (last !== undefined && found === last + 1) score += 8;
    if (found === 0 || BOUNDARY.test(t[found - 1])) score += 6;
    score -= Math.min(found - from, 10);
    indices.push(found);
    from = found + 1;
  }
  return { score, indices };
}
