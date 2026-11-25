// In a session's Terminal view, a run that has ended: how and when it
// ended, with Resume, as the app's empty state, above the screen it ended
// with, dimmed, while the host keeps it (a day). The screen is drawn by
// the terminal renderer at the size it had, read-only, and scaled down to
// fit the view.

import { Square, SquareTerminal } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { runtime, type Run } from "../../api/runtime";
import { endedText } from "../../lib/failedStart";
import { useSettings } from "../../state/settings";
import { age } from "../../lib/format";
import { agentOf } from "../AgentIcon";
import { EmptyState } from "../EmptyState";
import { RENDERERS } from "../terminal/renderers";
import { appearanceFrom } from "../terminal/terminalPool";
import { chooseRenderer, type TerminalView } from "../terminal/view";

type Screen = { paint: string; cols: number; rows: number };

/** The smallest the screen is drawn, where 13 px text reads at about
 *  9 px; a narrower view scrolls it sideways. */
const MIN_SCALE = 0.7;

const count = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/** How long a run lasted, in words: "40 seconds", "23 minutes", "1 hour
 *  5 minutes". */
export function lasted(from: string, to: string): string {
  const seconds = Math.max(0, Math.round((Date.parse(to) - Date.parse(from)) / 1000));
  if (seconds < 60) return count(seconds, "second");
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return count(minutes, "minute");
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `${count(hours, "hour")} ${count(rest, "minute")}` : count(hours, "hour");
}

export function EndedRun({
  run,
  onResume,
  resuming = false,
}: {
  run: Run;
  /** resumes the session, when it can be */
  onResume?: () => void;
  resuming?: boolean;
}) {
  const name = agentOf(run.agent)?.label ?? run.agent;
  // null while it loads; false once the host no longer keeps it. Kept
  // with its run, so another run's screen never shows while this one loads.
  const [loaded, setLoaded] = useState<{ run: string; screen: Screen | false } | null>(null);
  const screen = loaded?.run === run.id ? loaded.screen : null;

  useEffect(() => {
    let current = true;
    runtime.screen(run.id).then(
      (kept) => current && setLoaded({ run: run.id, screen: kept }),
      () => current && setLoaded({ run: run.id, screen: false }),
    );
    return () => {
      current = false;
    };
  }, [run.id]);

  const when = run.ended_at ? `It ended ${age(run.ended_at)} ago, after ${lasted(run.started_at, run.ended_at)}.` : "";
  const kept = screen
    ? "Its last screen is below."
    : screen === false
      ? "Pinrail no longer keeps its last screen."
      : "";
  const resume = onResume ? "Resume it to go on where it left off." : "";
  return (
    <div className={`ended-run ${screen ? "has-screen" : ""}`} data-testid="ended-run">
      <EmptyState
        // the way it ended, as a title
        title={endedText(name, run).replace(/\.$/, "")}
        icon={
          run.end_reason === "stopped" ? (
            <Square size={28} strokeWidth={1.5} />
          ) : (
            <SquareTerminal size={28} strokeWidth={1.5} />
          )
        }
        actions={
          onResume ? (
            <button type="button" className="chrome-button button-primary" disabled={resuming} onClick={onResume}>
              {resuming ? "Resuming…" : "Resume"}
            </button>
          ) : undefined
        }
      >
        {[when, kept, resume].filter(Boolean).join(" ")}
      </EmptyState>
      {screen ? <KeptScreen screen={screen} label={`${name}'s last screen`} /> : null}
    </div>
  );
}

/** The kept screen, drawn at its own size and scaled to the view's width. */
function KeptScreen({ screen, label }: { screen: Screen; label: string }) {
  const { settings } = useSettings();
  const renderers = settings.runtime.terminal.renderer;
  const area = useRef<HTMLDivElement>(null);
  const frame = useRef<HTMLDivElement>(null);
  const surface = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const host = surface.current!;
    let view: TerminalView | null = null;
    let gone = false;
    let natural = { width: 0, height: 0 };
    // the whole screen in the view's width, down to the smallest scale
    // that reads; a taller or still wider one scrolls
    const fit = () => {
      const box = area.current;
      if (!box || !natural.width) return;
      const style = getComputedStyle(box);
      const width = box.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
      const scale = Math.max(MIN_SCALE, Math.min(1, width / natural.width));
      host.style.transform = `scale(${scale})`;
      frame.current!.style.width = `${natural.width * scale}px`;
      frame.current!.style.height = `${natural.height * scale}px`;
    };
    const observer = new ResizeObserver(fit);
    // the app's theme changed: the screen takes its colours
    const restyle = () => view?.setTheme(appearanceFrom(document.body).theme);
    window.addEventListener("pinrail:appearance", restyle);
    (async () => {
      const renderer = await chooseRenderer(renderers, RENDERERS);
      if (gone) return;
      view = renderer.create();
      await view.mount(host, {
        cols: screen.cols,
        rows: screen.rows,
        scrollback: 1000,
        ...appearanceFrom(document.body),
      });
      // the cell the renderer draws with gives the screen's own size
      const [cellWidth, cellHeight] = view.measure().cellPx;
      natural = { width: screen.cols * cellWidth, height: screen.rows * cellHeight };
      host.style.width = `${natural.width}px`;
      host.style.height = `${natural.height}px`;
      view.resize(screen.cols, screen.rows);
      await view.restore("vt", new TextEncoder().encode(screen.paint));
      if (gone) return;
      fit();
      observer.observe(area.current!);
    })();
    return () => {
      gone = true;
      observer.disconnect();
      window.removeEventListener("pinrail:appearance", restyle);
      view?.dispose();
    };
  }, [screen, renderers]);

  return (
    <div ref={area} className="ended-run-screen" role="img" aria-label={label}>
      <div ref={frame} className="ended-run-frame">
        <div ref={surface} className="ended-run-surface" />
      </div>
    </div>
  );
}
