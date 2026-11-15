// The run of a session that Pinrail started: the live one with its
// terminal, or the latest when it exited with an error as it started,
// kept up to date as runs start, move between sessions and end.

import { useCallback, useEffect, useState } from "react";
import { runtime, type Run } from "../../api/runtime";
import { failedAtStart } from "../../lib/failedStart";
import { useNoticeCount } from "../../state/live";

export type SessionRun = {
  /** the live run with a terminal */
  live: Run | null;
  /** the latest run, when it exited with an error as it started */
  failed: Run | null;
  /** the latest run, when it ended otherwise */
  ended: Run | null;
  /** every run the session has been in, latest first */
  runs: Run[];
};

const NONE: SessionRun = { live: null, failed: null, ended: null, runs: [] };

const RUN_EVENTS = new Set(["run_started", "run_bound", "run_ended"]);

/** The session's run, and `reload`, which asks for it again at once, as
 *  after starting one. `enabled` is the agent runtime's switch: without
 *  it the window asks for no runs. */
export function useSessionRun(
  session: { agent: string; id: string } | null,
  enabled: boolean,
): SessionRun & { reload: () => void } {
  const key = session ? `${session.agent}/${session.id}` : null;
  const notices = useNoticeCount((notice) => RUN_EVENTS.has(notice.kind));
  const [reloads, setReloads] = useState(0);
  const reload = useCallback(() => setReloads((n) => n + 1), []);
  const changes = notices + reloads;
  const [found, setFound] = useState<{ key: string; run: SessionRun } | null>(null);

  useEffect(() => {
    if (!key || !enabled) return;
    let current = true;
    runtime.runs({ session: key }).then(
      ({ runs }) => {
        // a run moved to another session, as on /clear, still lists this one
        const here = runs.filter((r) => r.session && `${r.session.agent}/${r.session.id}` === key);
        const live = here.find((r) => r.state !== "ended" && r.terminal) ?? null;
        // runs come latest first
        const latest = !live && here[0]?.state === "ended" ? here[0] : null;
        const failed = latest && failedAtStart(latest) ? latest : null;
        const ended = latest && !failed ? latest : null;
        if (current) setFound({ key, run: { live, failed, ended, runs: here } });
      },
      () => {
        if (current) setFound({ key, run: NONE });
      },
    );
    return () => {
      current = false;
    };
  }, [key, enabled, changes]);

  return { ...(enabled && found?.key === key ? found.run : NONE), reload };
}
