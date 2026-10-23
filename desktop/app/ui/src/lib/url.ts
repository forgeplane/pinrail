// Reading and writing the query string in the address, for the screens whose
// filters and page live there.

import { useCallback, useRef } from "react";
import { useSearchParams } from "react-router";

/**
 * The query as the last render saw it, a way to change it that also sees
 * the changes just made, and a way to read the query with those changes.
 *
 * `useSearchParams` hands every render a snapshot and its setter merges into
 * that same snapshot, so two changes made before the router has re-rendered
 * both start from the old query and the second one undoes the first: two
 * quick clicks on *Previous* step back one page, not two. The write here
 * starts from what the URL is about to be instead.
 */
export function useUrlParams() {
  const [params, setParams] = useSearchParams();
  // what the URL will hold, including writes the router has not shown us yet
  const latest = useRef(params);
  // the address moved by itself: a link, the back button, another screen
  const seen = useRef(params);
  if (seen.current !== params) {
    seen.current = params;
    latest.current = params;
  }

  const update = useCallback(
    (change: (next: URLSearchParams) => void, options?: { replace?: boolean }) => {
      const next = new URLSearchParams(latest.current);
      change(next);
      latest.current = next;
      setParams(next, options);
    },
    [setParams],
  );

  /** The query the URL holds or is about to hold, which a render may not show yet. */
  const current = useCallback(() => latest.current, []);

  return [params, update, current] as const;
}

/** Drops every filter: `updateParams(clearAll, { replace: true })`. */
export function clearAll(next: URLSearchParams) {
  for (const key of [...next.keys()]) next.delete(key);
}

/** The route that opens a review: the inbox's side panel, or the review's own page. */
export const reviewPath = (id: string, openIn: "panel" | "page") =>
  openIn === "panel" ? `/?open=${encodeURIComponent(id)}` : `/reviews/${id}`;

/** The review a location shows, on its own page or in the inbox's side panel. */
export function reviewShown(pathname: string, search: string): string | null {
  const page = pathname.match(/^\/reviews\/([^/]+)/)?.[1];
  if (page) return decodeURIComponent(page);
  return pathname === "/" ? new URLSearchParams(search).get("open") : null;
}

/** The `repo` filter's value for the reviews that name no project. */
export const NO_PROJECT = "-";

/** Whether a review passes the project filter: none, a project, or `NO_PROJECT`. */
export const inProject = (repo: string | null | undefined, filter: string) =>
  !filter || (filter === NO_PROJECT ? !repo : repo === filter);
