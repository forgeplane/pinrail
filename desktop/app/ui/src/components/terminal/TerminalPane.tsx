// A hosted terminal in the window: the pooled view in the pane, attached
// once the pane has its size, and the bar under it with the viewer's role
// and the terminal's size.

import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { runtime } from "../../api/runtime";
import { useSettings } from "../../state/settings";
import { RENDERERS } from "./renderers";
import { appearanceFrom, TerminalPool, type PooledTerminal, type TerminalState } from "./terminalPool";
import { chooseRenderer } from "./view";

let rendererSetting: readonly string[] = ["auto"];
let shared: TerminalPool | undefined;

/** The window's pool of attached terminals. */
export function terminalPool(): TerminalPool {
  if (shared) return shared;
  const pool = new TerminalPool({
    renderer: () => chooseRenderer(rendererSetting, RENDERERS),
    appearance: () => appearanceFrom(document.body),
    ticket: runtime.ticket,
    url: runtime.attachUrl,
  });
  // the terminals follow the app's theme
  window.addEventListener("pinrail:appearance", () => pool.restyle());
  shared = pool;
  return pool;
}

const none = () => () => {};

export function TerminalPane({ terminal }: { terminal: string }) {
  const host = useRef<HTMLDivElement>(null);
  const { settings } = useSettings();
  const renderer = settings.runtime.terminal.renderer;
  useEffect(() => {
    rendererSetting = renderer;
  }, [renderer]);
  const [entry, setEntry] = useState<PooledTerminal | null>(null);
  const [failed, setFailed] = useState<string | null>(null);

  useEffect(() => {
    const pane = host.current!;
    const pool = terminalPool();
    let shown: PooledTerminal | null = null;
    let started = false;
    let gone = false;
    // the terminal attaches after the pane's first layout, so the snapshot
    // comes at the size it shows
    const observer = new ResizeObserver(() => {
      if (pane.clientWidth === 0 || pane.clientHeight === 0) return;
      if (started) return shown?.refit();
      started = true;
      pool.show(terminal, pane).then(
        (pooled) => {
          if (gone) return;
          shown = pooled;
          setEntry(pooled);
        },
        (error: unknown) => {
          if (!gone) setFailed(error instanceof Error ? error.message : String(error));
        },
      );
    });
    observer.observe(pane);
    return () => {
      gone = true;
      observer.disconnect();
      pool.hide(terminal);
      setEntry(null);
      setFailed(null);
    };
  }, [terminal]);

  const state = useSyncExternalStore(
    entry ? (listener) => entry.subscribe(listener) : none,
    () => entry?.state ?? null,
  );

  return (
    <div className="terminal-pane">
      <div className="terminal-host" ref={host} />
      <TerminalBar state={state} failed={failed} />
    </div>
  );
}

function TerminalBar({ state, failed }: { state: TerminalState | null; failed: string | null }) {
  let status: string;
  if (failed) status = `The terminal could not be shown: ${failed}`;
  else if (!state || state.stream === "connecting") status = "Connecting…";
  else if (state.exit) status = exitText(state.exit);
  else if (state.stream === "reconnecting") status = "Reconnecting…";
  else if (state.stream === "closed") status = "Disconnected from the terminal.";
  else if (state.role === "controller") status = "You are typing";
  else if (state.pty && state.fit && (state.pty.cols !== state.fit.cols || state.pty.rows !== state.fit.rows)) {
    status = "Size set by another viewer. Click to take control.";
  } else status = "Viewing. Click or type to take control.";
  const grid = state?.role === "controller" ? state.fit : (state?.pty ?? state?.fit);
  return (
    <div className="terminal-bar" data-testid="terminal-bar">
      <span role="status" aria-live="polite">
        {status}
      </span>
      {grid ? (
        <span className="terminal-size">
          {grid.cols} × {grid.rows}
        </span>
      ) : null}
    </div>
  );
}

function exitText(exit: NonNullable<TerminalState["exit"]>): string {
  if (exit.code !== undefined) return `Exited with code ${exit.code}.`;
  if (exit.signal !== undefined) return `Ended by signal ${exit.signal}.`;
  return "The program ended.";
}
