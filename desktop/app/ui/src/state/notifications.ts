// What macOS will do with the app's notifications, asked when a screen that
// shows it opens and again each time the window comes back (from System
// Settings, say), and the one way to ask macOS for permission.

import { useCallback, useEffect, useState } from "react";
import { inTauri } from "../api/client";
import { isMac } from "../lib/keys";

/** What macOS reports for the app's notifications; null outside the app bundle. */
export type NotificationStatus = {
  authorization: "authorized" | "denied" | "not_determined" | "provisional";
  alert_style: "none" | "banner" | "alert";
  alerts: boolean;
  sound: boolean;
  badge: boolean;
  shows: boolean;
};
export type SystemState = { known: boolean; status: NotificationStatus | null };

const invoke = <T>(command: string) => import("@tauri-apps/api/core").then(({ invoke }) => invoke<T>(command));

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

/** What the system will do with a notification, in a line for a settings row. */
export const describeSystem = ({ known, status }: SystemState) => {
  if (!inTauri())
    return isMac ? "What macOS allows shows here in the app" : "What your system allows shows here in the app";
  if (!known) return "…";
  // only macOS reports how it will show a notification
  if (!status)
    return isMac
      ? "Through the notification plugin in this development build; macOS reports nothing for it"
      : "Shown by your desktop, which does not report its settings to Pinrail";
  if (status.authorization === "denied") return "Not allowed in System Settings";
  if (status.authorization === "not_determined")
    return "Not yet allowed; macOS asks when you turn them on, or with the first one";
  if (status.alert_style === "none")
    return "Allowed, but the alert style is None in System Settings, so nothing appears";
  if (!status.alerts) return "Allowed, but alerts are off in System Settings";
  const parts = [
    status.alert_style === "alert" ? "Alerts" : "Banners",
    status.sound ? "sound on" : "sound off in System Settings",
    status.badge ? "badge" : "no badge",
  ];
  return `Allowed: ${parts.join(", ")}`;
};
