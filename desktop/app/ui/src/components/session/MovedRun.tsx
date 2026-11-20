// In a session's Terminal view, a session whose agent went on in a new
// session in the same terminal, as Claude Code does after /clear. The
// terminal is now in the new session, which this links to.

import { SquareTerminal } from "lucide-react";
import { Link } from "react-router";
import type { RunSession } from "../../api/runtime";
import { agentOf } from "../AgentIcon";
import { EmptyState } from "../EmptyState";
import { sessionPath } from "../Thread";

export function MovedRun({ agent, to }: { agent: string; to: RunSession }) {
  const name = agentOf(agent)?.label ?? agent;
  return (
    <div className="outside-run" data-testid="moved-run">
      <EmptyState
        title={`${name} went on in a new session`}
        icon={<SquareTerminal size={28} strokeWidth={1.5} />}
        actions={
          <Link className="chrome-button button-primary" to={sessionPath(to)}>
            Open the new session
          </Link>
        }
      >
        {`${name} started a new session in the same terminal, as it does after /clear. The terminal is now in the new session.`}
      </EmptyState>
    </div>
  );
}
