// The views of a session, as a segmented control of icons, each named by
// its tooltip.

import { ListCollapse, Rows2, SquareTerminal, type LucideIcon } from "lucide-react";
import { useRef } from "react";
import type { SessionView } from "../../lib/sessionView";
import { Tooltip } from "../Tooltip";

const VIEWS: [SessionView, string, LucideIcon][] = [
  ["summary", "Summary", ListCollapse],
  ["terminal", "Terminal", SquareTerminal],
  ["split", "Split view", Rows2],
];

export function ViewSwitch({ view, onView }: { view: SessionView; onView: (view: SessionView) => void }) {
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);
  return (
    <div className="view-switch" role="tablist" aria-label="Session view" data-testid="view-switch">
      {VIEWS.map(([id, label, Icon], i) => (
        <Tooltip key={id} label={label} keys={["Ctrl", "`"]} side="bottom">
          <button
            ref={(el) => {
              tabs.current[i] = el;
            }}
            type="button"
            role="tab"
            id={`view-tab-${id}`}
            aria-label={label}
            aria-selected={view === id}
            aria-controls="session-view"
            tabIndex={view === id ? 0 : -1}
            className={view === id ? "is-active" : undefined}
            onClick={() => onView(id)}
            onKeyDown={(event) => {
              if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
              event.preventDefault();
              const next = VIEWS[(i + 1) % VIEWS.length][0];
              onView(next);
              tabs.current[(i + 1) % VIEWS.length]?.focus();
            }}
          >
            <Icon size={15} aria-hidden="true" />
          </button>
        </Tooltip>
      ))}
    </div>
  );
}
