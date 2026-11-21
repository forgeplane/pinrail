// What a notification's row says: why it is there, and who caused it.

import type { Notification } from "../api/types";

/** The sentence that says why a notification is there, with who caused it. */
export function reasonOf(n: Notification): string {
  const who = n.actor?.kind === "agent" ? n.actor.name : n.actor?.kind === "system" ? "Pinrail" : "An agent";
  const status = n.subject.kind === "item" ? n.subject.item.status : null;
  switch (n.reason) {
    case "review_requested":
      return "A review waits for you";
    case "new_round":
      return "A new round waits for you";
    case "question_asked":
      return "An agent asked you";
    case "permission_requested":
      return "An agent waits for permission";
    case "input_requested":
      return "An agent waits for your answer";
    case "review_withdrawn":
      return "The agent withdrew this review";
    case "review_expired":
      return "This review expired before you decided";
    case "item_added":
      // a new sub-item is reported on its parent, with its key and title
      return n.summary ? `${who} added a sub-item` : `${who} added this work item`;
    case "item_commented":
      return `${who} commented`;
    case "item_closed":
      return status === "cancelled" ? `${who} cancelled this` : `${who} marked this done`;
    case "item_changed":
      return `${who} changed this work item`;
    case "agent_finished":
      return "The agent finished";
    case "agent_failed":
      return "The agent's turn failed";
    case "agent_stopped":
      return "The agent stopped";
    case "agent_note":
      return `${who} sent you a note`;
  }
}
