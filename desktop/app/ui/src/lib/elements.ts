// The chemical elements sessions are named after, loaded once from the
// core and kept for the window's life.

import { useEffect, useState } from "react";
import { api } from "../api/client";
import type { Element } from "../api/types";

let loaded: Element[] | null = null;
let loading: Promise<Element[]> | null = null;

/** The 118 elements by atomic number, once they have loaded. */
export function useElements(): Element[] | null {
  const [elements, setElements] = useState(loaded);
  useEffect(() => {
    if (loaded) return;
    let current = true;
    loading ??= api.elements().then(
      (d) => (loaded = d.elements),
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
