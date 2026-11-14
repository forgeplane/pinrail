// Stop, in the header of a session whose agent runs in Pinrail. While the
// agent works on a turn, Stop asks first, because it ends that turn.

import { Square } from "lucide-react";
import { useRef, useState } from "react";
import { errorMessage } from "../../api/client";
import { runtime, type Run } from "../../api/runtime";
import { agentOf } from "../AgentIcon";
import { Dialog } from "../Dialog";
import { Tooltip } from "../Tooltip";

export function StopButton({
  run,
  working,
  keys,
  asking,
  onAsking,
}: {
  run: Run;
  /** whether the agent works on a turn now */
  working: () => boolean;
  /** the keys that stop it, for the tooltip */
  keys: string[];
  /** whether the confirmation shows, which the session's key opens too */
  asking: boolean;
  onAsking: (asking: boolean) => void;
}) {
  const name = agentOf(run.agent)?.label ?? run.agent;
  const [stopping, setStopping] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const keep = useRef<HTMLButtonElement>(null);

  const stop = async () => {
    setStopping(true);
    setProblem(null);
    try {
      await runtime.stop(run.id);
      onAsking(false);
    } catch (e) {
      setProblem(errorMessage(e));
    } finally {
      setStopping(false);
    }
  };

  return (
    <>
      <Tooltip label={`Stop ${name}`} keys={keys} side="bottom">
        <button
          type="button"
          className="bar-button run-stop"
          aria-label={`Stop ${name}`}
          disabled={stopping}
          onClick={() => (working() ? onAsking(true) : void stop())}
        >
          <Square size={13} />
          <span>Stop</span>
        </button>
      </Tooltip>
      {asking ? (
        <Dialog labelledBy="stop-title" className="discard-dialog" initialFocus={keep} onClose={() => onAsking(false)}>
          <div className="dialog-head">
            <h2 id="stop-title">Stop {name}?</h2>
          </div>
          <p className="dim">
            {name} is working on a turn. Stopping it ends the turn, and {name} exits.
          </p>
          {problem ? <p className="notice notice-danger">{problem}</p> : null}
          <div className="dialog-actions">
            <button ref={keep} type="button" className="chrome-button" onClick={() => onAsking(false)}>
              Keep it running
            </button>
            <button
              type="button"
              className="chrome-button button-danger"
              disabled={stopping}
              onClick={() => void stop()}
            >
              {stopping ? "Stopping…" : "Stop"}
            </button>
          </div>
        </Dialog>
      ) : null}
    </>
  );
}
