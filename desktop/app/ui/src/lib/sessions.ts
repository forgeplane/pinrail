// A session's timeline: its questions and reviews, and between them the
// statuses the agent reported, in the order they happened. Statuses in a
// row with nothing between them are one group, and the agent saying it
// starts a stage, or has finished, is a line of its own.

import type { Review, SessionComment, SessionItemEvent, SessionMessage, SessionStatus } from "../api/types";

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
  | { kind: "message"; message: SessionMessage }
  | { kind: "comment"; comment: SessionComment }
  | { kind: "itemEvents"; events: SessionItemEvent[] };

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

/** The items, the statuses, what the agent did to work items and its
 *  comments on them, and the person's messages, oldest first, by their
 *  times, which the core keeps to the microsecond. Within one time, a
 *  withdrawn review's line goes first, then a review, a status, a work item
 *  change, a comment and a message: older times, kept to the second, share
 *  one often.
 *
 *  A review shows once, by its newest round, where its first round was
 *  sent. A later round adds a line where it was sent, unless only
 *  statuses came between it and the round before. A review the agent
 *  withdrew adds a line by the time it did. */
export function timeline(
  items: Review[],
  statuses: SessionStatus[],
  messages: SessionMessage[] = [],
  comments: SessionComment[] = [],
  itemEvents: SessionItemEvent[] = [],
): Entry[] {
  const rounds = roundsOf(items);
  type Happened =
    | { kind: "withdrawn"; review: Review }
    | { kind: "review"; review: Review }
    | { kind: "status"; status: SessionStatus }
    | { kind: "itemEvent"; event: SessionItemEvent }
    | { kind: "comment"; comment: SessionComment }
    | { kind: "message"; message: SessionMessage };
  const RANK = { withdrawn: 0, review: 1, status: 2, itemEvent: 3, comment: 4, message: 5 } as const;
  const all: { at: number; happened: Happened }[] = [];
  for (const review of items) {
    all.push({ at: Date.parse(review.created_at), happened: { kind: "review", review } });
    // a line where the agent withdrew it. A new round withdraws the one it
    // replaces, which needs no line, and a question says so itself
    if (review.status !== "withdrawn" || !review.withdrawn_at || review.kind === "question") continue;
    const chain = rounds.get(review.id)!;
    const next = chain[chain.indexOf(review) + 1];
    if (next && review.withdrawn_reason === `superseded by ${next.id}`) continue;
    all.push({ at: Date.parse(review.withdrawn_at), happened: { kind: "withdrawn", review } });
  }
  for (const status of statuses) all.push({ at: Date.parse(status.at), happened: { kind: "status", status } });
  for (const event of itemEvents) all.push({ at: Date.parse(event.at), happened: { kind: "itemEvent", event } });
  for (const comment of comments)
    all.push({ at: Date.parse(comment.created_at), happened: { kind: "comment", comment } });
  for (const message of messages) all.push({ at: Date.parse(message.sent_at), happened: { kind: "message", message } });
  // a stable sort keeps each list's own order within one time
  all.sort((a, b) => a.at - b.at || RANK[a.happened.kind] - RANK[b.happened.kind]);

  const entries: Entry[] = [];
  let group: SessionStatus[] = [];
  const close = () => {
    if (group.length) entries.push({ kind: "statuses", statuses: group });
    group = [];
  };
  for (const { happened } of all) {
    if (happened.kind === "status") {
      const status = happened.status;
      if (status.kind === "step") group.push(status);
      else {
        close();
        entries.push({
          kind: status.kind === "start" ? "started" : status.kind === "note" ? "note" : "finished",
          status,
        });
      }
      continue;
    }
    close();
    if (happened.kind === "comment") entries.push({ kind: "comment", comment: happened.comment });
    else if (happened.kind === "itemEvent") {
      // the same change to several items in a row is one line
      const last = entries[entries.length - 1];
      const same = (e: SessionItemEvent) => e.kind === happened.event.kind && e.attrs?.to === happened.event.attrs?.to;
      if (last?.kind === "itemEvents" && same(last.events[0])) last.events.push(happened.event);
      else entries.push({ kind: "itemEvents", events: [happened.event] });
    } else if (happened.kind === "message") entries.push({ kind: "message", message: happened.message });
    else if (happened.kind === "withdrawn") {
      const review = happened.review;
      entries.push({ kind: "withdrawn", review, card: rounds.get(review.id)!.at(-1)! });
    } else {
      const review = happened.review;
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
          (before?.kind === "item" && before.review === newest) ||
          (before?.kind === "revised" && before.card === newest);
        if (!follows) entries.push({ kind: "revised", review, round: counted ? n : null, card: newest });
      }
    }
  }
  close();
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
