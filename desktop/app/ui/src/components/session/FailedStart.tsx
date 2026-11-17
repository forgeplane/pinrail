// In a session's Terminal view, a run whose agent exited with an error as
// it started: the exit, its last lines, and starting it again as it was
// or after an edit. A resume that failed offers a new session in the
// folder instead, since the old one could not be resumed.

import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { errorMessage } from "../../api/client";
import { runtime, type Run } from "../../api/runtime";
import { exitText } from "../../lib/failedStart";
import { agentOf } from "../AgentIcon";
import type { NewSessionInitial } from "../NewSession";
import { sessionPath } from "../Thread";
import { startRun } from "./startRun";

export function FailedStart({ run }: { run: Run }) {
  const navigate = useNavigate();
  const name = agentOf(run.agent)?.label ?? run.agent;
  const [output, setOutput] = useState<string | null>(null);
  const [launch, setLaunch] = useState<{ task: string | null; args: string[] } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

  useEffect(() => {
    let current = true;
    runtime.output(run.id).then(
      (text) => current && setOutput(text),
      () => current && setOutput(""),
    );
    runtime.run(run.id).then(
      (full) => current && setLaunch({ task: full.task ?? null, args: full.args ?? [] }),
      () => current && setLaunch({ task: null, args: [] }),
    );
    return () => {
      current = false;
    };
  }, [run.id]);

  const resumed = run.kind === "resume";

  /** Starts the agent in the run's folder: as it was, or afresh. */
  const again = async (fresh: boolean) => {
    if (!launch && !fresh) return;
    setProblem(null);
    setStarting(true);
    try {
      const started = await startRun({
        agent: run.agent,
        folder: run.folder,
        task: fresh ? undefined : (launch!.task ?? undefined),
        args: fresh ? [] : launch!.args,
        local_id: crypto.randomUUID(),
      });
      if ("session" in started) navigate(sessionPath(started.session), { state: { newRun: true } });
      else if ("failed" in started) setProblem(exitText(name, started.failed));
      else setProblem(`${name} started, but has not reported its session.`);
    } catch (e) {
      setProblem(errorMessage(e));
    } finally {
      setStarting(false);
    }
  };

  const edit = () => {
    const initial: NewSessionInitial = {
      folder: run.folder,
      agent: run.agent,
      task: launch?.task ?? undefined,
      args: launch?.args,
    };
    window.dispatchEvent(new CustomEvent("pinrail:start-agent", { detail: initial }));
  };

  return (
    <div className="failed-start" data-testid="failed-start">
      <p className="failed-start-what" role="status">
        {exitText(name, run)}
      </p>
      {output ? (
        <pre className="failed-start-output" aria-label={`${name}'s last output`}>
          {output}
        </pre>
      ) : output === "" ? (
        <p className="dim">{name} printed nothing.</p>
      ) : null}
      {problem ? (
        <p className="notice notice-danger" role="alert">
          {problem}
        </p>
      ) : null}
      <div className="failed-start-actions">
        {resumed ? (
          <button
            type="button"
            className="chrome-button button-primary"
            disabled={starting}
            onClick={() => void again(true)}
          >
            {starting ? `Starting ${name}…` : "Start a new session in this folder"}
          </button>
        ) : (
          <>
            <button
              type="button"
              className="chrome-button button-primary"
              disabled={!launch || starting}
              onClick={() => void again(false)}
            >
              {starting ? `Starting ${name}…` : "Try again"}
            </button>
            <button type="button" className="chrome-button" disabled={!launch || starting} onClick={edit}>
              Edit and try again
            </button>
          </>
        )}
      </div>
    </div>
  );
}
