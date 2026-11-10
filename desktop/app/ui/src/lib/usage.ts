// The features of the window that the app's anonymous usage data counts.
// The core counts each once a day and only while usage data is on, so a
// report here is cheap and may be refused or lost without harm.

import { api } from "../api/client";

/** The features the window reports; the core accepts only these. */
export type Feature =
  | "command_palette"
  | "find_in_thread"
  | "list_filter"
  | "item_picker"
  | "quick_add"
  | "page_inbox"
  | "page_sessions"
  | "page_items"
  | "page_projects"
  | "page_history";

// what this window reported today, so a feature used often is sent once
let reported = { day: "", features: new Set<Feature>() };

/** Reports that the person used `feature`. */
export function countFeature(feature: Feature) {
  const day = new Date().toDateString();
  if (reported.day !== day) reported = { day, features: new Set() };
  if (reported.features.has(feature)) return;
  reported.features.add(feature);
  api.countFeature(feature).catch(() => {});
}

/** The page a location shows, as a feature, if it is one of the pages
 *  counted. */
export function pageFeature(pathname: string, search: string): Feature | null {
  if (pathname === "/") return new URLSearchParams(search).get("tab") === "history" ? "page_history" : "page_inbox";
  if (pathname === "/sessions" || pathname.startsWith("/sessions/")) return "page_sessions";
  if (pathname === "/items") return "page_items";
  if (pathname === "/projects") return "page_projects";
  return null;
}
