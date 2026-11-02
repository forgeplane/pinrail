// A session's timeline: its questions and reviews, and between them the
// statuses the agent reported, in the order they happened. Statuses in a
// row with nothing between them are one group, and the agent saying it
// starts a stage, or has finished, is a line of its own.

import type { Review, SessionMessage, SessionStatus } from "../api/types";

/** The router state of a review opened from a session: its breadcrumb
 *  names the session, and its decision leads back to the session. */
export type FromSession = { from: "session"; session: { agent: string; id: string; title: string | null } };

export const fromSession = (s: { agent: string; id: string; title?: string | null }): FromSession => ({
  from: "session",
  session: { agent: s.agent, id: s.id, title: s.title ?? null },
});

/** Which round a review's card shows, of how many. */
export type Round = { n: number; of: number };

export type Entry =
  | { kind: "item"; review: Review; round?: Round }
  | { kind: "revised"; review: Review; round: number | null; card: Review }
  | { kind: "withdrawn"; review: Review; card: Review }
  | { kind: "statuses"; statuses: SessionStatus[] }
  | { kind: "started"; status: SessionStatus }
  | { kind: "finished"; status: SessionStatus }
  | { kind: "note"; status: SessionStatus }
  | { kind: "message"; message: SessionMessage };

/** Each review's rounds among the items, oldest first, by the id of each
 *  of its rounds. A round that revises one from elsewhere starts a review. */
function roundsOf(items: Review[]): Map<string, Review[]> {
  const out = new Map<string, Review[]>();
  for (const review of items) {
    const chain = (review.revises && out.get(review.revises)) || [];
    chain.push(review);
    out.set(review.id, chain);
  }
  return out;
}

/** The newest round of each review, and the questions, in their order. */
export function newestRounds(items: Review[]): Review[] {
  const rounds = roundsOf(items);
  return items.filter((review) => rounds.get(review.id)!.at(-1) === review);
}

/** The items, the statuses and the person's messages, oldest first. A
 *  message goes after the status the core says was the newest when it was
 *  sent, and by its time among the items. A status goes after the
 *  review the core says it came after, which the times, in whole seconds,
 *  do not always tell. A status without one goes by its time, after a
 *  review of the same second.
 *
 *  A review shows once, by its newest round, where its first round was
 *  sent. A later round adds a line where it was sent, unless only
 *  statuses came between it and the round before. A review the agent
 *  withdrew adds a line by the time it did. */
export function timeline(items: Review[], statuses: SessionStatus[], messages: SessionMessage[] = []): Entry[] {
  const rounds = roundsOf(items);
  const index = new Map(items.map((review, i) => [review.id, i]));
  /** The last item sent at or before a time, or -1 before them all. */
  const slotAt = (at: string) => {
    let slot = -1;
    items.forEach((review, i) => {
      if (Date.parse(review.created_at) <= Date.parse(at)) slot = i;
    });
    return slot;
  };
  // the messages after each item, by its index, as the statuses below;
  // filled once the statuses have theirs, which a message can follow
  const said = new Map<number, SessionMessage[]>();
  const statusSlot = new Map<number, number>();
  // the statuses after each item, by its index; -1 is before them all
  const slots = new Map<number, SessionStatus[]>();
  for (const status of statuses) {
    let slot: number;
    if (status.after === null) slot = -1;
    else if (status.after !== undefined && index.has(status.after)) slot = index.get(status.after)!;
    else slot = slotAt(status.at);
    slots.set(slot, [...(slots.get(slot) ?? []), status]);
    statusSlot.set(status.id, slot);
  }
  for (const message of messages) {
    const after = message.after_status != null ? statusSlot.get(message.after_status) : undefined;
    const slot = Math.max(slotAt(message.sent_at), after ?? -1);
    said.set(slot, [...(said.get(slot) ?? []), message]);
  }
  // the reviews the agent withdrew after each item. A new round withdraws
  // the one it replaces, which needs no line, and a question says so itself
  const withdrawn = new Map<number, Review[]>();
  for (const review of items) {
    if (review.status !== "withdrawn" || !review.withdrawn_at || review.kind === "question") continue;
    const chain = rounds.get(review.id)!;
    const next = chain[chain.indexOf(review) + 1];
    if (next && review.withdrawn_reason === `superseded by ${next.id}`) continue;
    // times are in whole seconds: a later round came after it either way
    const at = slotAt(review.withdrawn_at);
    const slot = next ? Math.min(at, index.get(next.id)! - 1) : at;
    withdrawn.set(slot, [...(withdrawn.get(slot) ?? []), review]);
  }
  const entries: Entry[] = [];
  const place = (slot: number) => {
    let group: SessionStatus[] = [];
    const close = () => {
      if (group.length) entries.push({ kind: "statuses", statuses: group });
      group = [];
    };
    const queue = [...(said.get(slot) ?? [])];
    const ended = [...(withdrawn.get(slot) ?? [])].sort(
      (a, b) => Date.parse(a.withdrawn_at!) - Date.parse(b.withdrawn_at!),
    );
    const endUntil = (at: number) => {
      while (ended.length && Date.parse(ended[0].withdrawn_at!) <= at) {
        close();
        const review = ended.shift()!;
        entries.push({ kind: "withdrawn", review, card: rounds.get(review.id)!.at(-1)! });
      }
    };
    // the messages sent before a status, each a message of its own
    const sayUntil = (status: number) => {
      while (queue.length && (queue[0].after_status ?? -1) < status) {
        close();
        entries.push({ kind: "message", message: queue.shift()! });
      }
    };
    for (const status of slots.get(slot) ?? []) {
      endUntil(Date.parse(status.at));
      sayUntil(status.id);
      if (status.kind === "step") group.push(status);
      else {
        close();
        entries.push({
          kind: status.kind === "start" ? "started" : status.kind === "note" ? "note" : "finished",
          status,
        });
      }
    }
    close();
    sayUntil(Infinity);
    endUntil(Infinity);
  };
  place(-1);
  items.forEach((review, i) => {
    const chain = rounds.get(review.id)!;
    const [first, newest] = [chain[0], chain[chain.length - 1]];
    // the rounds are counted only from the first, which they are not when
    // it revises one from elsewhere
    const counted = !first.revises;
    const n = chain.indexOf(review) + 1;
    if (n === 1) {
      const round = counted && chain.length > 1 ? { n: chain.length, of: chain.length } : undefined;
      entries.push({ kind: "item", review: newest, round });
    } else {
      const before = [...entries].reverse().find((e) => e.kind !== "statuses");
      const follows =
        (before?.kind === "item" && before.review === newest) || (before?.kind === "revised" && before.card === newest);
      if (!follows) entries.push({ kind: "revised", review, round: counted ? n : null, card: newest });
    }
    place(i);
  });
  return entries;
}

/** A stage the agent started: its start, how it ended if it said, and
 *  whether it is the one the agent works on now. */
export type Stage = { start: SessionStatus; done: SessionStatus | null; current: boolean };

/** The stages the agent started, oldest first. A done ends the stage before
 *  it; a stage followed by another start ended without a word. */
export function stages(statuses: SessionStatus[]): Stage[] {
  const all: Stage[] = [];
  for (const status of statuses) {
    const open = all[all.length - 1];
    if (status.kind === "start") all.push({ start: status, done: null, current: false });
    else if (status.kind === "done" && open && !open.done) open.done = status;
  }
  const last = all[all.length - 1];
  if (last && !last.done) {
    const later = statuses.slice(statuses.indexOf(last.start) + 1);
    last.current = !later.some((s) => s.kind === "done");
  }
  return all;
}

/** The reviews each stage sent, by the id of the status that started it:
 *  the reviews after its start in the timeline, up to the next start.
 *  Questions are left out. */
export function stageReviews(entries: Entry[]): Map<number, Review[]> {
  const out = new Map<number, Review[]>();
  let stage: number | null = null;
  for (const entry of entries) {
    if (entry.kind === "started") {
      stage = entry.status.id;
      out.set(stage, []);
    } else if (entry.kind === "item" && stage !== null && entry.review.kind !== "question") {
      out.get(stage)!.push(entry.review);
    }
  }
  return out;
}
