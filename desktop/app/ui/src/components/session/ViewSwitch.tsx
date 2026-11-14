// The two views of a session, as a segmented control.

import { useRef } from "react";
import { combo, MOD, SHIFT } from "../../lib/keys";
import type { SessionView } from "../../lib/sessionView";
import { Tooltip } from "../Tooltip";

const VIEWS: [SessionView, string][] = [
  ["summary", "Summary"],
  ["terminal", "Terminal"],
];

export function ViewSwitch({ view, onView }: { view: SessionView; onView: (view: SessionView) => void }) {
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);
  return (
    <Tooltip label="Switch view" keys={["Ctrl", "`"]} side="bottom">
      <div className="view-switch" role="tablist" aria-label="Session view" data-testid="view-switch">
        {VIEWS.map(([id, label], i) => (
          <button
            key={id}
            ref={(el) => {
              tabs.current[i] = el;
            }}
            type="button"
            role="tab"
            id={`view-tab-${id}`}
            aria-selected={view === id}
            aria-controls="session-view"
            tabIndex={view === id ? 0 : -1}
            className={view === id ? "is-active" : undefined}
            title={id === "terminal" ? `Terminal (${combo(MOD, SHIFT, "T")})` : undefined}
            onClick={() => onView(id)}
            onKeyDown={(event) => {
              if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
              event.preventDefault();
              const next = VIEWS[(i + 1) % VIEWS.length][0];
              onView(next);
              tabs.current[(i + 1) % VIEWS.length]?.focus();
            }}
          >
            {label}
          </button>
        ))}
      </div>
    </Tooltip>
  );
}
