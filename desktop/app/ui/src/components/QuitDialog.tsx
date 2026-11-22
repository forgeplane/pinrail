// Quitting while agents run in Pinrail, with Settings › General › Quitting
// with agents running at Ask: the app keeps itself open and asks here
// whether to leave the agents running or to stop them. Do not ask again
// makes the choice the setting. Cancel and Esc keep Pinrail open.

import { Check } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { inTauri } from "../api/client";
import { AgentIcon, agentOf } from "./AgentIcon";
import { baseName } from "./NewSession";
import { Dialog } from "./Dialog";

/** The event in which the app asks, with the running agents. */
const ASK_EVENT = "pinrail:quit-asked";

/** A running agent, as the app lists it. */
export type RunningAgent = { agent: string; title: string | null; folder: string };

/** What names a running agent's session in the list. */
const nameOf = (r: RunningAgent) => r.title ?? `${agentOf(r.agent)?.label ?? r.agent} session`;

/** What happens to `n` running agents when Pinrail quits. */
const said = (n: number) =>
  n === 1
    ? "1 agent is running. It keeps running after Pinrail quits, and you can open it again when you reopen Pinrail."
    : `${n} agents are running. They keep running after Pinrail quits, and you can open them again when you reopen Pinrail.`;

export function QuitDialog({
  running,
  onAnswer,
  onClose,
}: {
  running: RunningAgent[];
  /** quit, after stopping the agents with `stop`; `remember` makes it the setting */
  onAnswer: (stop: boolean, remember: boolean) => Promise<void>;
  onClose: () => void;
}) {
  const [remember, setRemember] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const quit = useRef<HTMLButtonElement>(null);
  const n = running.length;

  const answer = async (stop: boolean) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await onAnswer(stop, remember);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  };

  return (
    <Dialog labelledBy="quit-title" className="discard-dialog quit-dialog" initialFocus={quit} onClose={onClose}>
      <div className="dialog-head">
        <h2 id="quit-title">Quit Pinrail?</h2>
      </div>
      <p className="dim">{said(n)}</p>
      <ul className="quit-dialog-agents" aria-label="Running agents">
        {running.map((r, i) => (
          <li key={i}>
            <AgentIcon requestedBy={r.agent} size={14} />
            <span className="truncate" title={nameOf(r)}>
              {nameOf(r)}
            </span>
            <span className="dim" title={r.folder}>
              {baseName(r.folder)}
            </span>
          </li>
        ))}
      </ul>
      {error ? <p className="notice notice-danger">{error}</p> : null}
      <div className="dialog-actions">
        <button
          type="button"
          role="checkbox"
          aria-checked={remember}
          className="quit-dialog-remember"
          onClick={() => setRemember((r) => !r)}
        >
          <span className="check-box" aria-hidden>
            {remember ? <Check size={12} strokeWidth={3} /> : null}
          </span>
          Do not ask again
        </button>
        <button type="button" className="chrome-button" onClick={onClose}>
          Cancel
        </button>
        <button type="button" className="chrome-button button-danger" disabled={busy} onClick={() => answer(true)}>
          Stop agents and quit
        </button>
        <button
          ref={quit}
          type="button"
          className="chrome-button button-primary"
          disabled={busy}
          onClick={() => answer(false)}
        >
          Quit
        </button>
      </div>
    </Dialog>
  );
}

/** The quit dialog, while the app asks. Outside the app it never shows. */
export function QuitAsk() {
  const [running, setRunning] = useState<RunningAgent[] | null>(null);

  useEffect(() => {
    if (!inTauri()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(async ({ listen }) => {
      const stop = await listen<{ running: RunningAgent[] }>(ASK_EVENT, (event) => setRunning(event.payload.running));
      if (cancelled) stop();
      else unlisten = stop;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  if (!running) return null;
  return (
    <QuitDialog
      running={running}
      onAnswer={async (stop, remember) => {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("quit", { stop, remember });
      }}
      onClose={() => setRunning(null)}
    />
  );
}
