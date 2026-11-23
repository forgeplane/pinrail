// The folders the setup's last screen offers to start a session in: those
// the core lists as recent, which are the ones Pinrail has seen and the
// ones the agents worked in before it. They come from here alone.

import { runtime } from "../../api/runtime";

/** A folder to start in, and what tells it from another of the same name. */
export type StartFolder = {
  folder: string;
  repo: string | null;
  branch: string | null;
  /** the agent whose own history names it, for one Pinrail has not seen */
  agent: string | null;
};

/** The folders to offer, the most recently used first: those that are
 *  still there. None when they cannot be read. */
export async function startFolders(): Promise<StartFolder[]> {
  try {
    return (await runtime.recentFolders())
      .filter((f) => f.exists)
      .map(({ folder, repo, branch, agent }) => ({ folder, repo, branch, agent: agent ?? null }));
  } catch {
    return [];
  }
}
