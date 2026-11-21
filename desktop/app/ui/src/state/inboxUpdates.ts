// How a notice changes the inbox's Updates: the person's notifications that
// do not wait for them, newest first. Kept apart from the inbox so it can be
// tested on its own.

import type { Notification } from "../api/types";

/** Whether the Updates section lists a notification: one about a work
 *  item that is not waiting or done. Reviews and sessions keep their own
 *  rows above it. */
export const listed = (n: Notification) => !n.waiting && !n.done && n.subject.kind === "item";

/**
 * The updates after a notice that carries a notification as it now stands:
 * a listed one takes its place by its latest event, newest first, and one
 * that is done or waits leaves the section.
 */
export function applyNotification(updates: Notification[], notification: Notification): Notification[] {
  const others = updates.filter((n) => n.id !== notification.id);
  if (!listed(notification)) return others;
  return [...others, notification].sort((a, b) => b.last_event - a.last_event);
}

/** The sentence that says why a notification is there, with who caused it. */
export function reasonOf(n: Notification): string {
  const who = n.actor?.kind === "agent" ? n.actor.name : n.actor?.kind === "system" ? "Pinrail" : "An agent";
  const status = n.subject.kind === "item" ? n.subject.item.status : null;
  switch (n.reason) {
    case "item_added":
      // a new sub-item is reported on its parent, with its key and title
      return n.summary ? `${who} added a sub-item` : `${who} added this work item`;
    case "item_commented":
      return `${who} commented`;
    case "item_closed":
      return status === "cancelled" ? `${who} cancelled this` : `${who} marked this done`;
    default:
      return `${who} changed this work item`;
  }
}
