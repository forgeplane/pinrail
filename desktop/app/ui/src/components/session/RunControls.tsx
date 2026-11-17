// Stop, in the header of a session whose agent runs in Pinrail, and
// Resume, in the header of one whose agent stopped. While the agent works
// on a turn, Stop asks first, because it ends that turn.

import { Play, Square } from "lucide-react";
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

/** Resume, for a stopped session whose agent can resume. */
export function ResumeButton({
  agent,
  resuming,
  onResume,
}: {
  agent: string;
  resuming: boolean;
  onResume: () => void;
}) {
  const name = agentOf(agent)?.label ?? agent;
  return (
    <Tooltip label={`Resume ${name} in Pinrail`} side="bottom">
      <button
        type="button"
        className="bar-button run-resume"
        aria-label={`Resume ${name}`}
        disabled={resuming}
        onClick={onResume}
      >
        <Play size={13} />
        <span>{resuming ? "Resuming…" : "Resume"}</span>
      </button>
    </Tooltip>
  );
}

/** Open in Pinrail, for a session still running in another terminal, with
 *  the warning that it starts a second process of the agent. */
export function OpenInPinrail({
  agent,
  opening,
  asking,
  onAsking,
  onOpen,
  unsure = false,
}: {
  agent: string;
  /** whether nothing tells if it still runs in another terminal */
  unsure?: boolean;
  opening: boolean;
  /** whether the warning shows, which the Terminal view's button opens too */
  asking: boolean;
  onAsking: (asking: boolean) => void;
  onOpen: () => void;
}) {
  const name = agentOf(agent)?.label ?? agent;
  const cancel = useRef<HTMLButtonElement>(null);
  return (
    <>
      <Tooltip label={`Open ${name} in Pinrail`} side="bottom">
        <button
          type="button"
          className="bar-button run-resume"
          aria-label={`Open ${name} in Pinrail`}
          disabled={opening}
          onClick={() => onAsking(true)}
        >
          <Play size={13} />
          <span>{opening ? "Opening…" : "Open in Pinrail"}</span>
        </button>
      </Tooltip>
      {asking ? (
        <Dialog
          labelledBy="open-title"
          className="discard-dialog"
          initialFocus={cancel}
          onClose={() => onAsking(false)}
        >
          <div className="dialog-head">
            <h2 id="open-title">Open {name} in Pinrail?</h2>
          </div>
          <p className="dim">
            {unsure
              ? `Pinrail cannot tell whether ${name} still runs in another terminal. If it does, opening it in Pinrail starts a second ${name} on this session, and both answer it. Close the other terminal first, or open it here anyway.`
              : `${name} is still running in another terminal. Opening it in Pinrail starts a second ${name} on this session, and both answer it. Close the other terminal first, or open it here anyway.`}
          </p>
          <div className="dialog-actions">
            <button ref={cancel} type="button" className="chrome-button" onClick={() => onAsking(false)}>
              Cancel
            </button>
            <button
              type="button"
              className="chrome-button button-primary"
              disabled={opening}
              onClick={() => {
                onAsking(false);
                onOpen();
              }}
            >
              Open anyway
            </button>
          </div>
        </Dialog>
      ) : null}
    </>
  );
}
