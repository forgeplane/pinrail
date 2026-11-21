// What a notification's row says: why it is there, and who caused it.

import type { Notice, Notification } from "../api/types";

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

/** What an event says on a notification's unfolded row, after who did it:
 *  "commented", with the start of the comment as its detail. `null` for an
 *  event not worth a line, such as an agent at work. */
export function eventLine(notice: Notice): { text: string; detail?: string } | null {
  const item = notice.item && "change" in notice.item ? notice.item : null;
  if (notice.kind === "item_changed" && item) {
    switch (item.change) {
      case "created":
        return item.parent
          ? { text: "added this work item under", detail: item.parent }
          : { text: "added this work item" };
      case "commented":
        return { text: "commented", detail: item.excerpt };
      case "status":
        return { text: `moved it to ${STATUS_WORDS[item.to ?? ""] ?? item.to}` };
      case "held":
        return { text: "put it on hold" };
      case "claimed":
        return { text: "claimed this work item" };
      case "released":
        return { text: "released it" };
      case "queued":
        return { text: "queued it" };
      case "unqueued":
        return { text: "took it out of the queue" };
      case "moved":
        return { text: "moved it to another repository" };
      case "parent":
        return { text: "moved it under another work item" };
      case "depends":
        return { text: "changed what it depends on" };
      case "files":
        return { text: "changed its files" };
      case "reviews":
        return { text: "changed its reviews" };
      case "comment_edited":
        return { text: "edited a comment", detail: item.excerpt };
      case "comment_deleted":
        return { text: "deleted a comment" };
      default:
        return { text: "changed this work item" };
    }
  }
  const session = notice.session as
    | {
        presence?: { state?: string; wait?: { kind?: string; summary?: string | null; tool?: string | null } };
        end_kind?: string;
        status?: { kind?: string; text?: string };
      }
    | undefined;
  switch (notice.kind) {
    case "session_presence": {
      const presence = session?.presence;
      if (presence?.state === "needs_input") {
        const wait = presence.wait;
        return {
          text: wait?.kind === "permission" ? "asked for permission" : "waited for your answer",
          detail: wait?.summary ?? wait?.tool ?? undefined,
        };
      }
      return presence?.state === "stopped" ? { text: "stopped" } : null;
    }
    case "session_turn":
      return session?.end_kind === "done"
        ? { text: "finished a turn" }
        : session?.end_kind === "failed"
          ? { text: "failed a turn" }
          : null;
    case "session_status":
      return session?.status?.kind === "note"
        ? { text: "sent you a note" }
        : { text: "said", detail: session?.status?.text };
    case "run_ended":
      return { text: "its run ended" };
    default:
      return null;
  }
}

const STATUS_WORDS: Record<string, string> = {
  open: "Open",
  in_progress: "In progress",
  on_hold: "On hold",
  done: "Done",
  cancelled: "Cancelled",
};

/** Who caused an event, by name, as its group on the unfolded row says. */
export const actorName = (notice: Notice) =>
  notice.actor?.kind === "agent" || notice.actor?.kind === "person"
    ? notice.actor.name
    : notice.actor?.kind === "system"
      ? "Pinrail"
      : "Someone";
