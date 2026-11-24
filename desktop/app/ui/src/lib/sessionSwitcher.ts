// The command palette's Sessions list with nothing typed: the sessions that
// need the person first, then those that finished a turn and those that
// work, and the rest folded under Earlier. Pure functions of the sessions,
// so the palette only draws what they return.

import type { Session } from "../api/types";
import { stateOf, waitingCount } from "./sidebarSessions";

/** The sections of the list, in order. */
export type Section = "needs_you" | "finished" | "working" | "earlier";

export const SECTION_LABEL: Record<Section, string> = {
  needs_you: "Needs you",
  finished: "Finished",
  working: "Working",
  earlier: "Earlier",
};

/** At most this many sessions are listed under Earlier. */
export const EARLIER_SHOWN = 20;

/** The section a session is listed under. */
export function sectionOf(session: Session): Section {
  const state = stateOf(session);
  return state === "needs_you" || state === "finished" || state === "working" ? state : "earlier";
}

const newest = (a: string | null | undefined, b: string | null | undefined) => (b ?? "").localeCompare(a ?? "");

/** When a session began to wait for the person, as near as it is known. */
const waitingSince = (s: Session) => (s.presence?.state === "needs_input" ? s.presence.since : s.last_at);

/**
 * The sessions in each section, in the order listed: the longest wait
 * first, then the latest finished turn, the latest activity, and for the
 * rest the latest activity too. Archived sessions are left out.
 */
export function sections(sessions: Session[]): Record<Section, Session[]> {
  const out: Record<Section, Session[]> = { needs_you: [], finished: [], working: [], earlier: [] };
  for (const s of sessions) if (!s.archived_at) out[sectionOf(s)].push(s);
  out.needs_you.sort((a, b) => waitingSince(a).localeCompare(waitingSince(b)));
  out.finished.sort((a, b) => newest(a.presence?.finished_at ?? a.last_at, b.presence?.finished_at ?? b.last_at));
  out.working.sort((a, b) => newest(a.last_at, b.last_at));
  out.earlier.sort((a, b) => newest(a.last_at, b.last_at));
  return out;
}

const count = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

/** What a session that needs the person waits for, such as "Bash · npm
 *  test" or "2 questions", or null when it needs nothing. */
export function waitText(session: Session): string | null {
  if (stateOf(session) !== "needs_you") return null;
  const { questions, reviews } = session.waiting;
  if (waitingCount(session)) {
    return [questions ? count(questions, "question") : null, reviews ? count(reviews, "review") : null]
      .filter(Boolean)
      .join(", ");
  }
  const wait = session.presence?.wait;
  if (wait?.kind === "permission") {
    const what = [wait.tool, wait.summary].filter(Boolean).join(" · ");
    return what || "A permission";
  }
  if (wait?.kind === "auth") return "A sign-in";
  return "A question";
}

/** The session a page shows, from its path, or null on another page. */
export function sessionShown(pathname: string): { agent: string; id: string } | null {
  const match = /^\/sessions\/([^/]+)\/([^/]+)/.exec(pathname);
  if (!match || match[1] === "new") return null;
  return { agent: decodeURIComponent(match[1]), id: decodeURIComponent(match[2]) };
}
