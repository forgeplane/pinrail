// A run whose agent exited with an error as it started: within 2 seconds,
// with a code other than 0 or a signal. Its session shows why, and offers
// to start it again.

import type { Run } from "../api/runtime";

/** How soon an exit counts as one at the start. */
export const AT_START_MS = 2000;

export function failedAtStart(run: Run): boolean {
  if (run.state !== "ended" || !run.ended_at) return false;
  if (run.end_reason === "stopped") return false;
  const failed = (run.exit_code !== null && run.exit_code !== 0) || run.exit_signal !== null;
  return failed && Date.parse(run.ended_at) - Date.parse(run.started_at) < AT_START_MS;
}

/** What the session says of the exit. */
export function exitText(name: string, run: Run): string {
  if (run.exit_code !== null) return `${name} exited with code ${run.exit_code} as it started.`;
  return `${name} was ended by signal ${run.exit_signal} as it started.`;
}
