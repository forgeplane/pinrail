// Loaded again, a list keeps the objects that did not change, so that what
// was drawn from them is not drawn again: a memoized component sees the same
// object, and a list with no change at all is the same list.

/** Whether two values parsed from JSON hold the same. */
export function sameValue(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) if (!sameValue(a[i], b[i])) return false;
    return true;
  }
  if (Array.isArray(b)) return false;
  const ka = Object.keys(a);
  if (ka.length !== Object.keys(b).length) return false;
  for (const k of ka) {
    if (!sameValue((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k])) return false;
  }
  return true;
}

/** `next`, with each object that holds the same as the one of its id in
 *  `prev` replaced by that one; `prev` itself when nothing changed. */
export function reuse<T>(prev: T[], next: T[], id: (value: T) => string | number): T[] {
  if (prev.length === 0) return next;
  const before = new Map(prev.map((value) => [id(value), value]));
  let changed = prev.length !== next.length;
  const out = next.map((value, i) => {
    const old = before.get(id(value));
    if (old !== undefined && sameValue(old, value)) {
      if (prev[i] !== old) changed = true;
      return old;
    }
    changed = true;
    return value;
  });
  return changed ? out : prev;
}

/** Whether two lists hold the same objects, in the same order. */
export const sameItems = <T>(a: readonly T[], b: readonly T[]) =>
  a === b || (a.length === b.length && a.every((value, i) => value === b[i]));
