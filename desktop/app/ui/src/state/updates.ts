// Where updating stands, as the app reports it: read once, then followed
// through the app's event. A development build and a browser report
// `unavailable`.

import { useCallback, useEffect, useState } from "react";
import { inTauri } from "../api/client";

export type UpdateStatus =
  | { state: "unavailable" }
  | { state: "idle" }
  | { state: "checking" }
  | { state: "up_to_date"; checked_at: string }
  | { state: "downloading"; version: string; percent: number | null }
  /** downloaded and checked; installed at the next restart */
  | { state: "ready"; version: string; notes: string | null }
  /** out, but for the package manager to install */
  | { state: "available"; version: string; url: string }
  | { state: "failed"; message: string; checked_at: string };

const invoke = <T>(command: string) => import("@tauri-apps/api/core").then(({ invoke }) => invoke<T>(command));

export function useUpdates() {
  const [status, setStatus] = useState<UpdateStatus>({ state: "unavailable" });

  useEffect(() => {
    if (!inTauri()) return;
    let cancelled = false;
    let stop: (() => void) | undefined;
    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const unlisten = await listen<UpdateStatus>("pinrail:update", (e) => setStatus(e.payload));
      if (cancelled) {
        unlisten();
        return;
      }
      stop = unlisten;
      invoke<UpdateStatus>("update_status")
        .then((s) => !cancelled && setStatus(s))
        .catch(() => {});
    })();
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  const check = useCallback(() => invoke<UpdateStatus>("check_for_updates").then(setStatus), []);
  const restart = useCallback(() => invoke<void>("restart_to_update"), []);
  return { status, check, restart };
}
