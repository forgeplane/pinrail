// Following an install or update job to its end, for the install panel and
// a plugin's row alike.

import { api } from "../api/client";
import type { InstallJob } from "../api/types";

const EVERY_MS = 300;
/** Polls that fail in a row before the job counts as lost. */
const TRIES = 4;

/**
 * Polls the job until it is done or failed, handing each step to `onStep`,
 * and returns the last state. A poll that fails, as during a restart, is
 * tried again with growing pauses; after `TRIES` in a row the job is
 * reported failed with what went wrong. `stopped` ends the following, as
 * when the panel closes, and then null comes back.
 */
export async function followJob(
  id: string,
  onStep: (job: InstallJob) => void,
  stopped: () => boolean,
): Promise<InstallJob | null> {
  let failures = 0;
  while (!stopped()) {
    try {
      const job = await api.pluginJob(id);
      failures = 0;
      if (job.status === "done" || job.status === "failed") return job;
      onStep(job);
    } catch (error) {
      failures++;
      if (failures >= TRIES) {
        const why = error instanceof Error ? error.message : String(error);
        return {
          id,
          source: "",
          status: "failed",
          log: "",
          plugin: null,
          error: `Its progress could not be read: ${why}`,
        };
      }
    }
    await new Promise((resolve) => window.setTimeout(resolve, EVERY_MS * 2 ** failures));
  }
  return null;
}
