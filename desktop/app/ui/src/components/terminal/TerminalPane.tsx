// A hosted terminal in the window: the pooled view in the pane, attached
// once the pane has its size, the bar under it with the viewer's role and
// the terminal's size, and find over it with Cmd+F (Ctrl+Shift+F on Linux).

import { KeyHints } from "../KeyHints";
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { runtime } from "../../api/runtime";
import type { Found } from "../../lib/findText";
import { isMac } from "../../lib/keys";
import { openAgentLink, revealFile } from "../../lib/native";
import { reachesApp } from "../../lib/terminalKeys";
import { useSettings } from "../../state/settings";
import { FindBar, useFindBar } from "../FindBar";
import { RENDERERS } from "./renderers";
import { appearanceFrom, TerminalPool, type PooledTerminal, type TerminalState } from "./terminalPool";
import { chooseRenderer } from "./view";

let rendererSetting: readonly string[] = ["auto"];
/** The terminal settings a view is made with. */
let viewSettings: {
  optionAsMeta: "off" | "left" | "right" | "both";
  screenReader: boolean;
  clipboardWrite: boolean;
} = {
  optionAsMeta: "off",
  screenReader: false,
  clipboardWrite: true,
};
let shared: TerminalPool | undefined;

/** The window's pool of attached terminals. */
export function terminalPool(): TerminalPool {
  if (shared) return shared;
  const pool = new TerminalPool({
    renderer: () => chooseRenderer(rendererSetting, RENDERERS),
    appearance: () => ({ ...appearanceFrom(document.body), ...viewSettings }),
    ticket: runtime.ticket,
    url: runtime.attachUrl,
  });
  // the terminals follow the app's theme
  window.addEventListener("pinrail:appearance", () => pool.restyle());
  shared = pool;
  return pool;
}

const none = () => () => {};

/** A hosted terminal; with `autoFocus`, it takes the keyboard once it is
 *  shown. */
export function TerminalPane({ terminal, autoFocus = false }: { terminal: string; autoFocus?: boolean }) {
  const host = useRef<HTMLDivElement>(null);
  const { settings } = useSettings();
  const renderer = settings.runtime.terminal.renderer;
  const {
    option_as_meta: optionAsMeta,
    screen_reader: screenReader,
    clipboard_write: clipboardWrite,
  } = settings.runtime.terminal;
  useEffect(() => {
    rendererSetting = renderer;
  }, [renderer]);
  useEffect(() => {
    const before = viewSettings;
    viewSettings = { optionAsMeta, screenReader, clipboardWrite };
    // terminals already shown take the change
    if (
      before.screenReader !== screenReader ||
      before.optionAsMeta !== optionAsMeta ||
      before.clipboardWrite !== clipboardWrite
    )
      terminalPool().restyle();
  }, [optionAsMeta, screenReader, clipboardWrite]);
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

  useEffect(() => {
    if (entry && autoFocus) entry.view.focus();
  }, [entry, autoFocus]);

  // a link Cmd-clicked (Ctrl-clicked on Linux) in the terminal opens
  useEffect(() => entry?.view.onLink((uri) => openTerminalLink(uri)), [entry]);

  // find in the screen and the scrollback: the next match is older, as the
  // newest output is at the bottom
  const { open: finding, focus: findFocus, query, setQuery, show: showFind, hide: hideFind } = useFindBar();
  const [found, setFound] = useState<Found | null>(null);
  const find = useCallback(
    (text: string, backwards?: boolean) => {
      if (!entry) return;
      const result = entry.view.search(text, { backwards });
      // the terminal counts its matches but does not say which is selected
      setFound(text ? { count: result.count ?? (result.found ? 1 : 0), current: 0 } : null);
    },
    [entry],
  );
  const closeFind = () => {
    entry?.view.search("", {});
    hideFind();
    setFound(null);
    entry?.view.focus();
  };

  // keys the terminal took stay with the program, except those the app
  // keeps: Cmd on macOS, Ctrl+Shift on Linux, and the view switch. Cmd+F
  // (Ctrl+Shift+F) opens find.
  useEffect(() => {
    const pane = host.current!;
    const keep = (event: KeyboardEvent) => {
      const findKey = isMac
        ? event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey
        : event.ctrlKey && event.shiftKey && !event.altKey && !event.metaKey;
      if (findKey && event.key.toLowerCase() === "f") {
        event.preventDefault();
        event.stopPropagation();
        showFind();
        return;
      }
      if (!reachesApp(event, isMac)) event.stopPropagation();
    };
    pane.addEventListener("keydown", keep);
    return () => pane.removeEventListener("keydown", keep);
  }, [showFind]);

  const state = useSyncExternalStore(
    entry ? (listener) => entry.subscribe(listener) : none,
    () => entry?.state ?? null,
  );

  return (
    <div className="terminal-pane">
      {finding && entry ? (
        <FindBar
          label="Find in the terminal"
          query={query}
          onQuery={setQuery}
          found={found}
          onFind={find}
          onClose={closeFind}
          focus={findFocus}
        />
      ) : null}
      <div className="terminal-host" ref={host} />
      <TerminalBar state={state} failed={failed} />
    </div>
  );
}

/** What the bar says of the terminal, and the tone of the dot before it:
 *  you type (ok), you view or it ended (idle), the link is coming
 *  (waiting), or it failed (danger). */
export function barStatus(
  state: TerminalState | null,
  failed: string | null,
): { text: string; tone: "ok" | "idle" | "waiting" | "danger" } {
  if (failed) return { text: `The terminal could not be shown: ${failed}`, tone: "danger" };
  if (!state || state.stream === "connecting") return { text: "Connecting…", tone: "waiting" };
  if (state.exit) return { text: exitText(state.exit), tone: "idle" };
  if (state.stream === "reconnecting") return { text: "Reconnecting to the terminal…", tone: "waiting" };
  if (state.stream === "closed") return { text: "Disconnected from the terminal.", tone: "danger" };
  if (state.role === "controller") return { text: "You are typing", tone: "ok" };
  if (state.pty && state.fit && (state.pty.cols !== state.fit.cols || state.pty.rows !== state.fit.rows)) {
    return { text: "Size set by another viewer · click to take control", tone: "idle" };
  }
  return { text: "Viewing · click or type to take control", tone: "idle" };
}

export function TerminalBar({ state, failed }: { state: TerminalState | null; failed: string | null }) {
  const status = barStatus(state, failed);
  const grid = state?.role === "controller" ? state.fit : (state?.pty ?? state?.fit);
  return (
    <div className="terminal-bar" data-testid="terminal-bar">
      <span className="terminal-status" role="status" aria-live="polite">
        <span className={`terminal-dot is-${status.tone}`} aria-hidden />
        {status.text}
      </span>
      <span className="terminal-bar-end">
        {grid ? (
          <span className="terminal-size">
            {grid.cols} × {grid.rows}
          </span>
        ) : null}
        <KeyHints className="terminal-hint" hints={[{ keys: ["Ctrl", "`"], label: "Summary" }]} />
      </span>
    </div>
  );
}

function exitText(exit: NonNullable<TerminalState["exit"]>): string {
  if (exit.code !== undefined) return `Exited with code ${exit.code}.`;
  if (exit.signal !== undefined) return `Ended by signal ${exit.signal}.`;
  return "The program ended.";
}

/** Opens a link from the terminal: a web or mail link in the system's
 *  browser or mail client, as the agent's own links, which may lead to a
 *  server on this machine; a file shown in its folder rather than opened,
 *  since a program's output can name any file, an app among them. */
export function openTerminalLink(uri: string) {
  if (/^file:/i.test(uri)) {
    let path: string;
    try {
      path = decodeURIComponent(new URL(uri).pathname);
    } catch {
      return;
    }
    revealFile(path);
  } else openAgentLink(uri);
}
