// In a session's Terminal view, a session Pinrail never ran: its agent ran
// in a terminal of the person's own. A stopped one can be resumed here.

import { agentOf } from "../AgentIcon";

export function OutsideRun({ agent, onResume, resuming }: { agent: string; onResume: () => void; resuming: boolean }) {
  const name = agentOf(agent)?.label ?? agent;
  return (
    <div className="outside-run" data-testid="outside-run">
      <p>This session ran in a terminal outside Pinrail.</p>
      <button type="button" className="chrome-button button-primary" disabled={resuming} onClick={onResume}>
        {resuming ? `Resuming ${name}…` : "Resume in Pinrail"}
      </button>
    </div>
  );
}
