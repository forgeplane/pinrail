// Starting an agent from the window, at the size its session's terminal
// will fit.

import { runtime, type RunSession, type StartRequest } from "../../api/runtime";
import { terminalPool } from "../terminal/TerminalPane";
import { terminalArea } from "../Thread";

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

/** Starts an agent, and answers its session, which the core makes at once. */
export async function startRun(request: Omit<StartRequest, "cols" | "rows">): Promise<RunSession> {
  const { session } = await runtime.start({ ...request, ...(await sessionGrid()) });
  return session;
}
