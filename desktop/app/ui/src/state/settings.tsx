// The settings the shell can change today, applied as they change. The
// appearance ones live in localStorage until the core keeps settings.json;
// launch-at-login and notifications are the app's, through commands.

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { inTauri } from "../api/client";
import { setThemePreference, themePreference, type ThemePreference } from "../lib/theme";

export type TextSize = "small" | "default" | "large";

export type Settings = {
  appearance: { theme: ThemePreference; text_size: TextSize };
  /** launch at login; null when the app cannot say (a browser) */
  autostart: boolean | null;
  notifications: { enabled: boolean | null };
};

type Patch = {
  appearance?: Partial<Settings["appearance"]>;
  autostart?: boolean;
  notifications?: Partial<Settings["notifications"]>;
};

type Store = { settings: Settings; update: (patch: Patch) => Promise<void>; native: boolean };

const KEY = "wicket:settings";
const ZOOM: Record<TextSize, string> = { small: "0.92", default: "1", large: "1.08" };

const SettingsContext = createContext<Store | null>(null);

function loadLocal(): Settings["appearance"] {
  let text_size: TextSize = "default";
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "{}") as { text_size?: TextSize };
    if (saved.text_size === "small" || saved.text_size === "large") text_size = saved.text_size;
  } catch {
    // nothing saved yet
  }
  return { theme: themePreference(), text_size };
}

export function applyTextSize(size: TextSize) {
  const root = document.getElementById("root");
  if (root) (root.style as CSSStyleDeclaration & { zoom: string }).zoom = ZOOM[size];
}

export function SettingsProvider({ children }: { children: ReactNode }) {
  const native = inTauri();
  const [settings, setSettings] = useState<Settings>(() => ({ appearance: loadLocal(), autostart: null, notifications: { enabled: null } }));

  useEffect(() => {
    applyTextSize(settings.appearance.text_size);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // what the app knows: launch at login, and whether notifications are paused
  useEffect(() => {
    if (!native) return;
    let stop: (() => void) | undefined;
    let cancelled = false;
    (async () => {
      const [{ invoke }, { listen }] = await Promise.all([import("@tauri-apps/api/core"), import("@tauri-apps/api/event")]);
      const [autostart, paused] = await Promise.all([invoke<boolean>("autostart_enabled").catch(() => null), invoke<boolean>("notifications_paused").catch(() => null)]);
      if (cancelled) return;
      setSettings((s) => ({ ...s, autostart, notifications: { enabled: paused === null ? null : !paused } }));
      // the tray's pause item changes the same flag
      const unlisten = await listen<boolean>("wicket:notifications", (e) => setSettings((s) => ({ ...s, notifications: { enabled: !e.payload } })));
      if (cancelled) unlisten();
      else stop = unlisten;
    })();
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [native]);

  const update = useCallback(
    async (patch: Patch) => {
      if (patch.appearance?.theme) setThemePreference(patch.appearance.theme);
      if (patch.appearance?.text_size) {
        applyTextSize(patch.appearance.text_size);
        try {
          localStorage.setItem(KEY, JSON.stringify({ text_size: patch.appearance.text_size }));
        } catch {
          // applies for this session
        }
      }
      if (native && (patch.autostart !== undefined || patch.notifications?.enabled !== undefined)) {
        const { invoke } = await import("@tauri-apps/api/core");
        if (patch.autostart !== undefined) await invoke("set_autostart", { enabled: patch.autostart });
        if (patch.notifications?.enabled !== undefined) await invoke("set_notifications_paused", { paused: !patch.notifications.enabled });
      }
      setSettings((s) => ({
        appearance: { ...s.appearance, ...patch.appearance },
        autostart: patch.autostart ?? s.autostart,
        notifications: { ...s.notifications, ...patch.notifications },
      }));
    },
    [native],
  );

  const value = useMemo(() => ({ settings, update, native }), [settings, update, native]);
  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}

export function useSettings(): Store {
  const store = useContext(SettingsContext);
  if (!store) throw new Error("useSettings outside SettingsProvider");
  return store;
}
