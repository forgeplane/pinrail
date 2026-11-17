// The chemical elements sessions are named after, loaded once from the
// core and kept for the window's life.

import { useEffect, useState } from "react";
import { api } from "../api/client";
import type { Element } from "../api/types";

let loaded: Element[] | null = null;
let loading: Promise<Element[]> | null = null;
// the elements some session has had, from the list and from the sessions
// seen since
let discovered: Set<number> | null = null;

/** The 118 elements by atomic number, once they have loaded. */
export function useElements(): Element[] | null {
  const [elements, setElements] = useState(loaded);
  useEffect(() => {
    if (loaded) return;
    let current = true;
    loading ??= api.elements().then(
      (d) => {
        loaded = d.elements;
        discovered = new Set(d.elements.filter((e) => (e.sessions ?? 0) > 0).map((e) => e.number));
        return loaded;
      },
      (e: unknown) => {
        loading = null;
        throw e;
      },
    );
    loading.then(
      (all) => current && setElements(all),
      () => {},
    );
    return () => {
      current = false;
    };
  }, []);
  return elements;
}

/** The element of an atomic number, from the list. */
export const elementOf = (elements: Element[] | null, number: number | null | undefined) =>
  number ? (elements?.[number - 1] ?? null) : null;

/** A category as a class name, such as "noble-gas". */
export const categoryClass = (category: string) => category.replace(/\s+/g, "-");

/** The elements of these sessions not in `known`, the elements some
 *  session had, which it adds them to: each with how many are known then. */
export function newlyDiscovered(
  known: Set<number>,
  elements: Element[],
  sessions: { element?: number | null }[],
): { element: Element; count: number }[] {
  const found: { element: Element; count: number }[] = [];
  for (const s of sessions) {
    if (!s.element || known.has(s.element)) continue;
    known.add(s.element);
    const element = elements[s.element - 1];
    if (element) found.push({ element, count: known.size });
  }
  return found;
}

/** The elements of these sessions no session had before, which are now
 *  discovered; none until the list has loaded. */
export const discover = (sessions: { element?: number | null }[]) =>
  loaded && discovered ? newlyDiscovered(discovered, loaded, sessions) : [];
