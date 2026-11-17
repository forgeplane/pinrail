// Resuming a session in Pinrail: which sessions can be resumed, which can
// be opened in Pinrail while they still run in another terminal, and what
// the window says when one cannot.

import { ApiError, errorMessage } from "../api/client";
import type { Run } from "../api/runtime";
import type { Session } from "../api/types";

type Resumable = Pick<Session, "presence" | "resume">;

/** Whether the session can be resumed: no run of it lives, its agent can
 *  resume, and it stopped, as its hooks said or its last run did. */
export function canResume(session: Resumable | null | undefined, runs: Run[]): boolean {
  if (!session?.resume?.resume) return false;
  if (runs.some((r) => r.state !== "ended")) return false;
  const presence = session.presence;
  if (presence) return presence.state === "stopped";
  return runs.length > 0;
}

/** Whether the session may run in another terminal and can be opened in
 *  Pinrail anyway, as a second process of its agent: no run of Pinrail's
 *  lives, its agent allows it, and its hooks say it has not stopped, or
 *  it has neither hooks nor a run of Pinrail's to tell. */
export function canOpenWhileRunning(session: Resumable | null | undefined, runs: Run[]): boolean {
  if (!session?.resume?.while_running) return false;
  if (runs.some((r) => r.state !== "ended")) return false;
  if (!session.presence) return runs.length === 0;
  return session.presence.state !== "stopped";
}

/** Why a resume did not happen, for the person. */
export function resumeProblem(error: unknown, folder: string | null): string {
  if (error instanceof ApiError && error.kind === "folder_missing" && folder) {
    return `The folder ${folder} no longer exists, so the session cannot be resumed here.`;
  }
  return errorMessage(error);
}
