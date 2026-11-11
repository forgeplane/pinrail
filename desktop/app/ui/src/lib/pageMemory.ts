// The query each main page was last left with, such as the Work items
// page's filters, so that its sidebar link and its shortcut bring the
// person back to the list as they left it. The open side panel is not
// kept, and the inbox's tab is not either: Inbox opens on its own tab.

import type { To } from "react-router";

/** The pages whose query is kept, by path. */
const PAGES = ["/", "/sessions", "/items", "/projects"];
/** What a page's query keeps of the address. */
const DROPPED: Record<string, string[]> = {
  "/": ["open", "tab"],
  "/sessions": ["open"],
  "/items": ["open"],
  "/projects": ["open"],
};

const remembered = new Map<string, string>();

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
