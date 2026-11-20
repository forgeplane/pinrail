// On the Sessions page after the computer restarted: how many agents the
// restart stopped, with Resume all, which resumes them one by one, and
// Review, which lists them. The banner stays until they are resumed or
// the person dismisses it.

import { X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router";
import { runtime, type Run } from "../../api/runtime";
import { resumeProblem } from "../../lib/resume";
import { useNoticeCount } from "../../state/live";
import { agentOf } from "../AgentIcon";
import { sessionPath } from "../Thread";
import { sessionGrid } from "./startRun";

type Stopped = Run & { title: string | null };

const RUN_EVENTS = new Set(["run_started", "run_bound", "run_ended"]);

/** What names a stopped session in the banner. */
function nameOf(run: Stopped): string {
  if (run.title) return run.title;
  const agent = agentOf(run.agent)?.label ?? run.agent;
  return `${agent} session ${run.session?.agent_session ?? run.id}`;
}

export function RestartBanner() {
  const changes = useNoticeCount((notice) => RUN_EVENTS.has(notice.kind));
  const [stopped, setStopped] = useState<Stopped[]>([]);
  const [reviewing, setReviewing] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);
  const [problems, setProblems] = useState<string[]>([]);

  const load = useCallback(() => {
    runtime.restarted().then(
      // a session whose agent never reported its own id cannot be resumed
      (runs) => setStopped(runs.filter((r) => r.session?.agent_session)),
      () => setStopped([]),
    );
  }, []);
  useEffect(load, [load, changes]);

  if (stopped.length === 0) return null;

  const resumeAll = async () => {
    const failed: string[] = [];
    const grid = await sessionGrid();
    for (const [i, run] of stopped.entries()) {
      setProgress(`Resuming ${i + 1} of ${stopped.length}…`);
      try {
        await runtime.resume(run.session!, { ...grid, local_id: crypto.randomUUID() });
      } catch (e) {
        failed.push(`${nameOf(run)}: ${resumeProblem(e, run.folder)}`);
      }
    }
    setProgress(null);
    setProblems(failed);
    load();
  };

  const dismiss = async () => {
    await runtime.dismissRestart().catch(() => undefined);
    setStopped([]);
  };

  const n = stopped.length;
  return (
    <div className="restart-banner" data-testid="restart-banner">
      <div className="restart-banner-head">
        <p role="status">
          {n} {n === 1 ? "agent" : "agents"} stopped when the computer restarted.
        </p>
        <button type="button" className="chrome-button" disabled={!!progress} onClick={() => void resumeAll()}>
          {progress ?? "Resume all"}
        </button>
        <button
          type="button"
          className="chrome-button"
          aria-expanded={reviewing}
          onClick={() => setReviewing((r) => !r)}
        >
          Review
        </button>
        <button type="button" className="bar-button" aria-label="Dismiss" onClick={() => void dismiss()}>
          <X size={14} />
        </button>
      </div>
      {problems.length ? (
        <p className="notice notice-danger" role="alert">
          {problems.join("\n")}
        </p>
      ) : null}
      {reviewing ? (
        <ul className="restart-banner-list">
          {stopped.map((run) => (
            <li key={run.id}>
              <Link to={sessionPath(run.session!)}>{nameOf(run)}</Link>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
