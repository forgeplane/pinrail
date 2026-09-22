// Routes the app sends the shell to: from the tray, the shortcut, a deep
// link or a notification. A route sent before the shell was listening is
// picked up at start. Outside the app this does nothing.

import { useEffect } from "react";
import { useNavigate } from "react-router";
import { inTauri } from "../api/client";

const OPEN_EVENT = "wicket:open";

/** On macOS the app draws its own title bar; the traffic lights overlay the top-left. */
export const overlayTitleBar = inTauri() && /Mac/i.test(navigator.platform);
const COMMAND_EVENT = "wicket:command";

/** The scheme of a link this app will follow. */
export const EXTERNAL = /^(https?|mailto):/i;

/** Opens a link outside the app: the system browser, or the mail client. */
export function openExternal(url: string) {
  if (!EXTERNAL.test(url)) return;
  if (!inTauri()) {
    window.open(url, "_blank", "noreferrer");
    return;
  }
  import("@tauri-apps/plugin-opener").then(({ openUrl }) => openUrl(url)).catch(() => window.open(url, "_blank"));
}

/** Inside the app a link to the outside world opens in the system browser. */
export function useExternalLinks() {
  useEffect(() => {
    if (!inTauri()) return;
    const onClick = (event: MouseEvent) => {
      const anchor = (event.target as HTMLElement | null)?.closest?.("a[href]") as HTMLAnchorElement | null;
      if (!anchor || event.defaultPrevented) return;
      const url = anchor.href;
      if (!EXTERNAL.test(url) || anchor.target !== "_blank") return;
      event.preventDefault();
      openExternal(url);
    };
    document.addEventListener("click", onClick);
    return () => document.removeEventListener("click", onClick);
  }, []);
}

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
      const stopOpen = await listen<string>(OPEN_EVENT, (event) => go(event.payload));
      // menu accelerators arrive as commands and are re-issued as a DOM
      // event, so the shell handles them like its own shortcuts
      const stopCommand = await listen<string>(COMMAND_EVENT, (event) => {
        window.dispatchEvent(new CustomEvent(COMMAND_EVENT, { detail: event.payload }));
      });
      const stop = () => {
        stopOpen();
        stopCommand();
      };
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
