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

/** A row's second line: what changed, the detail that says it, how many
 *  other changes there are, and the parent of a sub-item. */
export type RowLine = { text: string; detail?: string; more?: number; under?: string };

/** A line of Markdown as plain text: without its emphasis, code and link
 *  marks. */
export const plain = (text: string | undefined) =>
  text
    ?.replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/(\*\*|__|\*|_|`)(.+?)\1/g, "$2")
    .trim();

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** What a row's second line says: the latest change since the person
 *  looked, in words, with its detail and the count of the others. */
export function lineOf(n: Notification): RowLine {
  const who = n.actor?.kind === "agent" ? n.actor.name : n.actor?.kind === "system" ? "Pinrail" : "An agent";
  const fresh = n.new;
  const under = fresh?.parent ?? undefined;
  if (n.subject.kind === "session" && fresh && n.reason === "agent_finished") {
    return {
      text: fresh.turns > 1 ? `The agent finished ${fresh.turns} turns` : "The agent finished",
      detail: plain(fresh.reply ?? n.summary ?? undefined),
    };
  }
  if (n.subject.kind === "item" && fresh?.latest) {
    const total = fresh.sub_items + fresh.comments + fresh.changes;
    const line = (text: string, count: number, detail?: string): RowLine => ({
      text,
      detail,
      more: total - count || undefined,
      under,
    });
    switch (fresh.latest) {
      case "sub_item":
        if (fresh.breakdown) {
          return line(
            `${who} added this work item with ${plural(fresh.sub_items, "sub-item", "sub-items")}`,
            fresh.sub_items,
          );
        }
        return fresh.sub_items === 1
          ? line(
              `${who} added a sub-item`,
              1,
              fresh.sub_item ? `${fresh.sub_item.key} ${fresh.sub_item.title}` : undefined,
            )
          : line(`${who} added ${fresh.sub_items} sub-items`, fresh.sub_items);
      case "comment":
        return fresh.comments === 1
          ? line(`${fresh.authors[0] ?? who} commented`, 1, plain(fresh.comment ?? undefined) || undefined)
          : line(`${fresh.comments} new comments`, fresh.comments, fresh.authors.join(", "));
      case "change":
        return line(reasonOf(n), 1, n.summary ?? undefined);
    }
  }
  return { text: reasonOf(n), detail: n.summary ?? undefined, under };
}
