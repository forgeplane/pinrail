// The query each main page was last left with, such as the Work items
// page's filters and the item open in its side panel, so that its sidebar
// link and its shortcut bring the person back to the page as they left it.
// The inbox's tab is not kept: Inbox opens on its own tab. Each page's
// scroll position is kept too, by its address.

import type { To } from "react-router";

/** The pages whose query is kept, by path. */
const PAGES = ["/", "/sessions", "/items", "/projects"];
/** What a page's query keeps of the address. */
const DROPPED: Record<string, string[]> = {
  "/": ["tab"],
};

const remembered = new Map<string, string>();
const scrolls = new Map<string, number>();

/** Keeps the query of a main page the person is on. */
export function rememberPage(pathname: string, search: string) {
  if (!PAGES.includes(pathname)) return;
  const params = new URLSearchParams(search);
  for (const key of DROPPED[pathname] ?? []) params.delete(key);
  const kept = params.toString();
  if (kept) remembered.set(pathname, `?${kept}`);
  else remembered.delete(pathname);
}

/** Where a main page's link and shortcut go: the page with its last query. */
export function pageLink(pathname: string): To {
  return { pathname, search: remembered.get(pathname) ?? "" };
}

/** Keeps how far a main page's list is scrolled, at the address it is on. */
export function rememberScroll(pathname: string, search: string, top: number) {
  if (!PAGES.includes(pathname)) return;
  scrolls.set(pathname + search, top);
}

/** How far a main page's list was scrolled at this address, if it was. */
export function rememberedScroll(pathname: string, search: string): number | undefined {
  return scrolls.get(pathname + search);
}
