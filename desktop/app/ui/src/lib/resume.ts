// Resuming a session in Pinrail: which sessions can be resumed, which can
// be opened in Pinrail while they still run in another terminal, and what
// the window says when one cannot.

import { ApiError, errorMessage } from "../api/client";
import type { Run } from "../api/runtime";
import type { Session } from "../api/types";

type Resumable = Pick<Session, "presence" | "resume" | "agent_session">;

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
 *  Pinrail anyway, after a warning: Pinrail knows the agent's own id for
 *  it and no run of Pinrail's lives, and either its hooks say it has not
 *  stopped and its agent allows a second process, or it has neither hooks
 *  nor a run of Pinrail's to tell and its agent resumes. */
export function canOpenWhileRunning(session: Resumable | null | undefined, runs: Run[]): boolean {
  if (!session?.resume?.resume || !session.agent_session) return false;
  if (runs.some((r) => r.state !== "ended")) return false;
  if (!session.presence) return runs.length === 0;
  return !!session.resume.while_running && session.presence.state !== "stopped";
}

/** Why a resume did not happen, for the person. */
export function resumeProblem(error: unknown, folder: string | null): string {
  if (error instanceof ApiError && error.kind === "folder_missing" && folder) {
    return `The folder ${folder} no longer exists, so the session cannot be resumed here.`;
  }
  return errorMessage(error);
}
