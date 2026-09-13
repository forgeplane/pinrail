// Routes the app sends the shell to: from the tray, the shortcut, a deep
// link or a notification. A route sent before the shell was listening is
// picked up at start. Outside the app this does nothing.

import { useEffect } from "react";
import { useNavigate } from "react-router";
import { inTauri } from "../api/client";

const OPEN_EVENT = "wicket:open";

export function useNativeRoutes() {
  const navigate = useNavigate();
  useEffect(() => {
    if (!inTauri()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    (async () => {
      const [{ listen }, { invoke }] = await Promise.all([import("@tauri-apps/api/event"), import("@tauri-apps/api/core")]);
      const go = (route: string) => {
        if (route) navigate(route);
      };
      const stop = await listen<string>(OPEN_EVENT, (event) => go(event.payload));
      if (cancelled) {
        stop();
        return;
      }
      unlisten = stop;
      const pending = await invoke<string | null>("take_pending_route");
      if (pending && !cancelled) go(pending);
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [navigate]);
}
