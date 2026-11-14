// The live run of a session that Pinrail started, with its terminal, kept
// up to date as runs start, move between sessions and end.

import { useEffect, useState } from "react";
import { runtime, type Run } from "../../api/runtime";
import { useNoticeCount } from "../../state/live";

const RUN_EVENTS = new Set(["run_started", "run_bound", "run_ended"]);

/** The session's live run that has a terminal, or null. `enabled` is the
 *  agent runtime's switch: without it the window asks for no runs. */
export function useSessionRun(session: { agent: string; id: string } | null, enabled: boolean): Run | null {
  const key = session ? `${session.agent}/${session.id}` : null;
  const changes = useNoticeCount((notice) => RUN_EVENTS.has(notice.kind));
  const [found, setFound] = useState<{ key: string; run: Run | null } | null>(null);

  useEffect(() => {
    if (!key || !enabled) return;
    let current = true;
    runtime.runs({ state: "live", session: key }).then(
      ({ runs }) => {
        // a run moved to another session, as on /clear, still lists this one
        const run = runs.find((r) => r.terminal && r.session && `${r.session.agent}/${r.session.id}` === key);
        if (current) setFound({ key, run: run ?? null });
      },
      () => {
        if (current) setFound({ key, run: null });
      },
    );
    return () => {
      current = false;
    };
  }, [key, enabled, changes]);

  return enabled && found?.key === key ? found.run : null;
}
