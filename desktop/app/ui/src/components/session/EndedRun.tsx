// In a session's Terminal view, a run that has ended: how it ended, over
// its last screen, dimmed, while the host keeps it (a day).

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { runtime, type Run } from "../../api/runtime";
import { endedText } from "../../lib/failedStart";
import { agentOf } from "../AgentIcon";

/** How much of the ended terminal it shows. */
const LINES = 500;

export function EndedRun({ run }: { run: Run }) {
  const name = agentOf(run.agent)?.label ?? run.agent;
  // null while it loads; false once the host no longer keeps it
  const [screen, setScreen] = useState<string | false | null>(null);
  const shown = useRef<HTMLPreElement>(null);
  // the last screen is at the end
  useLayoutEffect(() => {
    const el = shown.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [screen]);

  useEffect(() => {
    let current = true;
    runtime.output(run.id, LINES).then(
      (text) => current && setScreen(text),
      () => current && setScreen(false),
    );
    return () => {
      current = false;
    };
  }, [run.id]);

  return (
    <div className="ended-run" data-testid="ended-run">
      <p className="ended-run-banner" role="status">
        {endedText(name, run)}
      </p>
      {screen ? (
        <pre ref={shown} className="ended-run-screen" aria-label={`${name}'s last screen`}>
          {screen}
        </pre>
      ) : screen === false ? (
        <p className="dim">Pinrail no longer keeps this run's last screen.</p>
      ) : null}
    </div>
  );
}
