// The repository the page shows, such as a session's or a review's, which
// the quick add dialog starts from. The page shown last wins, as a review
// opened beside a session's thread does, and a page's repository is
// forgotten when the page goes.

import { useEffect } from "react";

const named: { repo: string }[] = [];

/** Names the repository the calling page shows, while it shows it. */
export function usePageRepo(repo: string | null | undefined) {
  useEffect(() => {
    if (!repo) return;
    const entry = { repo };
    named.push(entry);
    return () => {
      const at = named.indexOf(entry);
      if (at >= 0) named.splice(at, 1);
    };
  }, [repo]);
}

/** The repository the pages shown now name, the last one first. */
export const pageRepo = () => named.at(-1)?.repo ?? null;
