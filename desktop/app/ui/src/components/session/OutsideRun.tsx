// In a session's Terminal view, a session whose agent runs, or ran, outside
// Pinrail. Pinrail did not start it, so it has no terminal and no Stop here,
// and the view says why. A stopped one can be resumed here, and a running
// one opened here, after a warning, where its agent allows it.

import { SquareTerminal } from "lucide-react";
import type { Session } from "../../api/types";
import { agentOf } from "../AgentIcon";
import { EmptyState } from "../EmptyState";

/** How the session stands outside Pinrail: it can be opened here while it
 *  runs, resumed here, or neither, while it runs in a terminal, after it
 *  stopped, or with no hooks to tell. */
export type Outside = "open" | "resume" | "running" | "stopped" | "statuses";

export function OutsideRun({
  agent,
  outside,
  session,
  unsure = false,
  onResume,
  resuming,
}: {
  agent: string;
  outside: Outside;
  /** what tells whether Pinrail could resume it once it stops */
  session?: Pick<Session, "resume" | "agent_session"> | null;
  /** with open: whether nothing tells if it still runs there */
  unsure?: boolean;
  onResume: () => void;
  resuming: boolean;
}) {
  const name = agentOf(agent)?.label ?? agent;
  const title = {
    open: unsure ? `${name} may run in another terminal` : `${name} runs in another terminal`,
    resume: `${name} ran in another terminal`,
    running: `${name} runs in another terminal`,
    stopped: `${name} ran in another terminal`,
    statuses: `${name} runs outside Pinrail`,
  }[outside];
  const noResume = !session?.resume?.resume
    ? `Pinrail cannot resume ${name} sessions.`
    : `${name} did not report its own id for this session, so Pinrail cannot resume it.`;
  const text = {
    open: unsure
      ? `Pinrail cannot tell whether this session still runs in a terminal outside Pinrail. Open it here to see its terminal and type to ${name}. Pinrail starts its own ${name} on the session, so close any other terminal it runs in first.`
      : `This session runs in a terminal outside Pinrail. Open it here to see its terminal and type to ${name}. Pinrail starts its own ${name} on the session, so close the other terminal first.`,
    resume: `This session ran in a terminal outside Pinrail. Resume it in Pinrail to continue it here, with its terminal in this view.`,
    running:
      `This session runs in a terminal outside Pinrail, so Pinrail cannot show its terminal or stop it. To stop ${name}, use the terminal it runs in.` +
      (session?.resume?.resume && session.agent_session ? " Once it stops, you can resume it in Pinrail." : ""),
    stopped: `This session ran in a terminal outside Pinrail. ${noResume}`,
    statuses: `Pinrail knows this session only from the statuses, questions and reviews that ${name} sends. Pinrail did not start it, so it cannot show its terminal or stop it.`,
  }[outside];
  const actionable = outside === "open" || outside === "resume";
  return (
    <div className="outside-run" data-testid="outside-run">
      <EmptyState
        title={title}
        icon={<SquareTerminal size={28} strokeWidth={1.5} />}
        actions={
          actionable ? (
            <button type="button" className="chrome-button button-primary" disabled={resuming} onClick={onResume}>
              {resuming
                ? `${outside === "open" ? "Opening" : "Resuming"} ${name}…`
                : outside === "open"
                  ? "Open in Pinrail"
                  : "Resume in Pinrail"}
            </button>
          ) : undefined
        }
      >
        {text}
      </EmptyState>
    </div>
  );
}
