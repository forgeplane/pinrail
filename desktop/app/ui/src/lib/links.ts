// Links a plugin's view asks the app to open. The app opens one only when
// the person allowed its origin for that plugin, or agrees when asked.
// Email links are always asked about, since their body can carry anything.

import type { LinkPermission, Plugin } from "../api/types";
import { followable } from "./native";

/** Longer than this, an address may carry a review's content out, so it is
 *  always asked about, even on an allowed origin. */
export const LONG_LINK = 2000;

export type LinkRequest = {
  url: string;
  kind: "web" | "mail";
  /** for a web address, its origin, such as https://github.com */
  origin: string | null;
  /** what the person is shown first: the host, in its ASCII form so a
   *  look-alike name shows as punycode, or the email's recipient */
  target: string;
  long: boolean;
};

/** A request the app would consider, or null for an address it never opens. */
export function linkRequest(url: string): LinkRequest | null {
  if (!followable(url)) return null;
  const parsed = new URL(url);
  const long = url.length > LONG_LINK;
  if (parsed.protocol === "mailto:") {
    let to = parsed.pathname;
    try {
      to = decodeURIComponent(to);
    } catch {
      // shown as written
    }
    return { url, kind: "mail", origin: null, target: to || "no recipient", long };
  }
  return { url, kind: "web", origin: parsed.origin, target: parsed.host, long };
}

/** Where a plugin came from, which its link permission is tied to. */
export const sourceOf = (plugin: Plugin): string =>
  plugin.install && plugin.install.source_kind !== "app" ? plugin.install.source : "built in";

/** Whether the request can open without asking. */
export function allowedWithoutAsking(
  request: LinkRequest,
  permission: LinkPermission | undefined,
  source: string,
): boolean {
  return (
    request.kind === "web" &&
    !request.long &&
    !!request.origin &&
    !!permission &&
    permission.source === source &&
    permission.origins.includes(request.origin)
  );
}

/** The permission after allowing one more origin; one from another source starts over. */
export function allowing(permission: LinkPermission | undefined, source: string, origin: string): LinkPermission {
  const kept = permission && permission.source === source ? permission.origins : [];
  return { source, origins: kept.includes(origin) ? kept : [...kept, origin] };
}
