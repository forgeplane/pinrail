// The rows of the inbox's Agents section: the sessions whose agent waits on
// the person in its terminal, and those that finished a turn the person has
// not seen. The questions and reviews an agent sends keep their own rows.

import type { Session } from "../api/types";
import { age } from "./format";

/** The most rows the section shows; the rest are on the Sessions page. */
export const AGENT_ROWS = 10;

export type AgentRow = { session: Session; reason: string; since: string };

export function agentRows(sessions: Session[], now: number): AgentRow[] {
  return sessions.flatMap((session): AgentRow[] => {
    const p = session.presence;
    if (!p) return [];
    if (session.attention === "finished") {
      const at = p.finished_at ?? p.since;
      return [{ session, reason: `Finished ${age(at, now)} ago`, since: at }];
    }
    if (session.attention === "needs_you" && p.state === "needs_input") {
      const reason =
        p.wait?.kind === "permission"
          ? `Waiting for permission${p.wait.tool ? `: ${p.wait.tool}` : ""}`
          : "Waiting for your answer";
      return [{ session, reason, since: p.since }];
    }
    return [];
  });
}
