// The sessions the window has seen in a list, such as the sidebar's, by
// agent and id: a session's page shows its header from this at once, while
// its thread loads.

import type { Session } from "../api/types";

const known = new Map<string, Session>();

/** Keeps the sessions of a list the window loaded. */
export function rememberSessions(sessions: Session[]) {
  for (const s of sessions) known.set(`${s.agent}/${s.id}`, s);
}

/** The session as a list last gave it, if one did. */
export const knownSession = (key: string | null) => (key ? (known.get(key) ?? null) : null);
