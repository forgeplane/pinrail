// A session's timeline: its questions and reviews, and between them the
// statuses the agent reported, in the order they happened. Statuses in a
// row with nothing between them are one group, and the agent saying it
// starts a stage, or has finished, is a line of its own.

import type { Review, SessionStatus } from "../api/types";

export type Entry =
  | { kind: "item"; review: Review }
  | { kind: "statuses"; statuses: SessionStatus[] }
  | { kind: "started"; status: SessionStatus }
  | { kind: "finished"; status: SessionStatus };

/** The items and the statuses, oldest first. A status goes after the
 *  review the core says it came after, which the times, in whole seconds,
 *  do not always tell. A status without one goes by its time, after a
 *  review of the same second. */
export function timeline(items: Review[], statuses: SessionStatus[]): Entry[] {
  const index = new Map(items.map((review, i) => [review.id, i]));
  // the statuses after each item, by its index; -1 is before them all
  const slots = new Map<number, SessionStatus[]>();
  for (const status of statuses) {
    let slot: number;
    if (status.after === null) slot = -1;
    else if (status.after !== undefined && index.has(status.after)) slot = index.get(status.after)!;
    else {
      slot = -1;
      items.forEach((review, i) => {
        if (Date.parse(review.created_at) <= Date.parse(status.at)) slot = i;
      });
    }
    slots.set(slot, [...(slots.get(slot) ?? []), status]);
  }
  const entries: Entry[] = [];
  const place = (slot: number) => {
    let group: SessionStatus[] = [];
    const close = () => {
      if (group.length) entries.push({ kind: "statuses", statuses: group });
      group = [];
    };
    for (const status of slots.get(slot) ?? []) {
      if (status.kind === "step") group.push(status);
      else {
        close();
        entries.push({ kind: status.kind === "start" ? "started" : "finished", status });
      }
    }
    close();
  };
  place(-1);
  items.forEach((review, i) => {
    entries.push({ kind: "item", review });
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
