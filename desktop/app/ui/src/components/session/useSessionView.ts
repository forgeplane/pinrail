// The view a session's page shows: Summary, Terminal, or both, the summary
// above the terminal. The address holds
// it as `?view=`, so back, forward and links keep it; without one, the
// setting decides, and the last view used for the session is remembered.

import { useCallback, useMemo, useState } from "react";
import { useLocation } from "react-router";
import { lastView, openingView, rememberView, type SessionView } from "../../lib/sessionView";
import { useUrlParams } from "../../lib/url";
import { useSettings } from "../../state/settings";

export function useSessionView(session: { agent: string; id: string } | null, hasTerminal: boolean) {
  const { settings } = useSettings();
  const [params, updateParams] = useUrlParams();
  const location = useLocation();
  const key = session ? `${session.agent}/${session.id}` : null;
  // a run the person just started opens in the view its setting gives
  const newRun = (location.state as { newRun?: boolean } | null)?.newRun === true;
  const setting = newRun ? settings.runtime.new_run_view : settings.runtime.default_view;
  const last = useMemo(() => (key ? lastView(key) : null), [key]);
  const view = openingView(params.get("view"), setting, last, hasTerminal);
  // how many times the person switched, so the page moves the focus
  const [switches, setSwitches] = useState(0);

  const choose = useCallback(
    (next: SessionView) => {
      if (!key) return;
      rememberView(key, next);
      updateParams((p) => p.set("view", next));
      setSwitches((n) => n + 1);
    },
    [key, updateParams],
  );
  // Ctrl+` takes the keyboard out of a terminal: from Terminal, or the
  // split, to the summary, and from the summary to the terminal
  const toggle = useCallback(() => choose(view === "summary" ? "terminal" : "summary"), [choose, view]);
  return { view, choose, toggle, switches };
}
