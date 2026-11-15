// Resuming a stopped session in Pinrail: which sessions can be resumed,
// and what the window says when one cannot.

import { ApiError, errorMessage } from "../api/client";
import type { Run } from "../api/runtime";
import type { Session } from "../api/types";

/** The agents whose sessions Pinrail resumes, as the core's adapters do. */
const RESUMABLE = new Set(["claude-code", "codex"]);

/** Whether the session can be resumed: no run of it lives, its agent can
 *  resume, and it stopped, as its hooks said or its last run did. */
export function canResume(session: Pick<Session, "agent" | "presence"> | null | undefined, runs: Run[]): boolean {
  if (!session || !RESUMABLE.has(session.agent)) return false;
  if (runs.some((r) => r.state !== "ended")) return false;
  const presence = session.presence;
  if (presence) return presence.state === "stopped";
  return runs.length > 0;
}

/** Why a resume did not happen, for the person. */
export function resumeProblem(error: unknown, folder: string | null): string {
  if (error instanceof ApiError && error.kind === "folder_missing" && folder) {
    return `The folder ${folder} no longer exists, so the session cannot be resumed here.`;
  }
  return errorMessage(error);
}
