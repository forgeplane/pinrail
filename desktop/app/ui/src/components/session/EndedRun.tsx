// In a session's Terminal view, a run that has ended: how it ended, over
// the screen it ended with, dimmed, while the host keeps it (a day). The
// screen is drawn by the terminal renderer at the size it had, read-only,
// and scaled down to fit the view.

import { useEffect, useRef, useState } from "react";
import { runtime, type Run } from "../../api/runtime";
import { endedText } from "../../lib/failedStart";
import { useSettings } from "../../state/settings";
import { agentOf } from "../AgentIcon";
import { RENDERERS } from "../terminal/renderers";
import { appearanceFrom } from "../terminal/terminalPool";
import { chooseRenderer, type TerminalView } from "../terminal/view";

type Screen = { paint: string; cols: number; rows: number };

/** The smallest the screen is drawn, where 13 px text reads at about
 *  9 px; a narrower view scrolls it sideways. */
const MIN_SCALE = 0.7;

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
  // null while it loads; false once the host no longer keeps it
  const [screen, setScreen] = useState<Screen | false | null>(null);

  useEffect(() => {
    let current = true;
    runtime.screen(run.id).then(
      (kept) => current && setScreen(kept),
      () => current && setScreen(false),
    );
    return () => {
      current = false;
    };
  }, [run.id]);

  return (
    <div className="ended-run" data-testid="ended-run">
      <div className="ended-run-banner">
        <p role="status">{endedText(name, run)}</p>
        {onResume ? (
          <button type="button" className="chrome-button" disabled={resuming} onClick={onResume}>
            {resuming ? "Resuming…" : "Resume"}
          </button>
        ) : null}
      </div>
      {screen ? (
        <KeptScreen screen={screen} label={`${name}'s last screen`} />
      ) : screen === false ? (
        <p className="dim ended-run-gone">Pinrail no longer keeps this run's last screen.</p>
      ) : null}
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
