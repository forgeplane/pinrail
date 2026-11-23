// The folders the setup's last screen offers to start a session in. They
// come from here alone, so another source, such as the folders an agent
// worked in before Pinrail, is added in one place.

import { runtime } from "../../api/runtime";

/** A folder to start in, and what tells it from another of the same name. */
export type StartFolder = { folder: string; repo: string | null; branch: string | null };

/** The folders to offer, the most likely first: those agents worked in
 *  lately from Pinrail that are still there. None when they cannot be read. */
export async function startFolders(): Promise<StartFolder[]> {
  try {
    return (await runtime.recentFolders())
      .filter((f) => f.exists)
      .map(({ folder, repo, branch }) => ({ folder, repo, branch }));
  } catch {
    return [];
  }
}
