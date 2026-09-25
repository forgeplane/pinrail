// What macOS will do with the app's notifications, asked when a screen that
// shows it opens and again each time the window comes back (from System
// Settings, say), and the one way to ask macOS for permission.

import { useCallback, useEffect, useState } from "react";
import { inTauri } from "../api/client";

/** What macOS reports for the app's notifications; null outside the app bundle. */
export type NotificationStatus = { authorization: "authorized" | "denied" | "not_determined" | "provisional"; alert_style: "none" | "banner" | "alert"; alerts: boolean; sound: boolean; badge: boolean; shows: boolean };
export type SystemState = { known: boolean; status: NotificationStatus | null };

const invoke = <T,>(command: string) => import("@tauri-apps/api/core").then(({ invoke }) => invoke<T>(command));

export function useNotificationStatus(open: boolean) {
  const [state, setState] = useState<SystemState>({ known: false, status: null });
  useEffect(() => {
    if (!open || !inTauri()) return;
    let cancelled = false;
    const ask = () =>
      invoke<NotificationStatus | null>("notification_status")
        .then((s) => !cancelled && setState({ known: true, status: s }))
        .catch(() => !cancelled && setState({ known: true, status: null }));
    ask();
    window.addEventListener("focus", ask);
    return () => {
      cancelled = true;
      window.removeEventListener("focus", ask);
    };
  }, [open]);

  /** macOS's prompt the first time; the answer already given after that */
  const request = useCallback(
    () =>
      invoke<NotificationStatus | null>("request_notifications")
        .then((s) => setState({ known: true, status: s }))
        .catch(() => {}),
    [],
  );
  const openSystemSettings = useCallback(() => invoke<void>("open_notification_settings").catch(() => {}), []);
  return { system: state, request, openSystemSettings };
}
