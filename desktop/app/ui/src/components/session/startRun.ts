// Starting an agent from the window: at the size its session's terminal
// will fit, then waiting, for an agent that names its session only once
// it starts, until it does, or until it exits.

import { runtime, type Run, type StartRequest } from "../../api/runtime";
import { failedAtStart } from "../../lib/failedStart";
import { terminalPool } from "../terminal/TerminalPane";
import { terminalArea } from "../Thread";

/** How long a start waits for an agent to name its session. */
const BIND_WITHIN_MS = 20_000;

/** The grid the new session's terminal will fit, as its renderer
 *  measures it in the space the session page will give it. */
export async function sessionGrid(): Promise<{ cols: number; rows: number }> {
  const main = document.querySelector(".app-main")?.getBoundingClientRect();
  const area = terminalArea(main ?? new DOMRect(0, 0, window.innerWidth, window.innerHeight));
  try {
    const { cols, rows } = await terminalPool().gridFor(area.width, area.height);
    return { cols: Math.min(1000, Math.max(20, cols)), rows: Math.min(1000, Math.max(5, rows)) };
  } catch {
    return { cols: 120, rows: 40 };
  }
}

export type Started =
  | { session: { agent: string; id: string } }
  /** it exited with an error before it named its session */
  | { failed: Run }
  /** it runs, but has not named its session */
  | { unnamed: Run };

/** Starts an agent, and waits for its session. */
export async function startRun(request: Omit<StartRequest, "cols" | "rows">): Promise<Started> {
  const { run, session } = await runtime.start({ ...request, ...(await sessionGrid()) });
  if (session) return { session };
  const until = Date.now() + BIND_WITHIN_MS;
  let now = run;
  while (Date.now() < until) {
    await new Promise((r) => setTimeout(r, 400));
    now = await runtime.run(run.id);
    if (now.session) return { session: now.session };
    if (now.state === "ended") return failedAtStart(now) ? { failed: now } : { unnamed: now };
  }
  return { unnamed: now };
}
