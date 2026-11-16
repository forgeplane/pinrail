// In a session's Terminal view, a session whose agent runs, or ran, in a
// terminal of the person's own. A stopped one can be resumed here, and a
// running one opened here, after a warning.

import { SquareTerminal } from "lucide-react";
import { agentOf } from "../AgentIcon";
import { EmptyState } from "../EmptyState";

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
      <EmptyState
        title={running ? `${name} runs in another terminal` : `${name} ran in another terminal`}
        icon={<SquareTerminal size={28} strokeWidth={1.5} />}
        actions={
          <button type="button" className="chrome-button button-primary" disabled={resuming} onClick={onResume}>
            {resuming
              ? `${running ? "Opening" : "Resuming"} ${name}…`
              : running
                ? "Open in Pinrail"
                : "Resume in Pinrail"}
          </button>
        }
      >
        {running
          ? `This session runs in a terminal outside Pinrail. Open it here to see its terminal and type to ${name}. Pinrail starts its own ${name} on the session, so close the other terminal first.`
          : `This session ran in a terminal outside Pinrail. Resume it in Pinrail to continue it here, with its terminal in this view.`}
      </EmptyState>
    </div>
  );
}
