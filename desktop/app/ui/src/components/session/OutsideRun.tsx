// In a session's Terminal view, a session whose agent runs, or ran, in a
// terminal of the person's own. A stopped one can be resumed here, and a
// running one opened here, after a warning.

import { agentOf } from "../AgentIcon";

export function OutsideRun({
  agent,
  running = false,
  onResume,
  resuming,
}: {
  agent: string;
  /** whether it still runs there */
  running?: boolean;
  onResume: () => void;
  resuming: boolean;
}) {
  const name = agentOf(agent)?.label ?? agent;
  return (
    <div className="outside-run" data-testid="outside-run">
      <p>This session {running ? "runs" : "ran"} in a terminal outside Pinrail.</p>
      <button type="button" className="chrome-button button-primary" disabled={resuming} onClick={onResume}>
        {resuming ? `${running ? "Opening" : "Resuming"} ${name}…` : running ? "Open in Pinrail" : "Resume in Pinrail"}
      </button>
    </div>
  );
}
