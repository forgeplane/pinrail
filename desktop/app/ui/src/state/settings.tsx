// The settings, from the core's settings.json through the API: read at
// start, applied as they arrive, changed with a PATCH, and followed live
// when something else changes them (the CLI, a hand edit, the tray). The
// last appearance is cached in localStorage for the first paint only.
// Launch-at-login and the notification pause are the app's own, through
// commands, until the core applies those keys itself.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api, inTauri } from "../api/client";
import type { ServerSettings } from "../api/types";
import { cacheAppearance, setThemePreference, type ThemePreference } from "../lib/theme";
import { useLive } from "./live";

export type TextSize = "small" | "default" | "large";

export type Settings = {
  appearance: { theme: ThemePreference; text_size: TextSize };
  sidebar: { open: boolean };
  /** launch at login; null when the app cannot say (a browser) */
  autostart: boolean | null;
  notifications: { enabled: boolean | null };
};

type Patch = {
  appearance?: Partial<Settings["appearance"]>;
  sidebar?: Partial<Settings["sidebar"]>;
  autostart?: boolean;
  notifications?: Partial<Settings["notifications"]>;
};

type Store = { settings: Settings; update: (patch: Patch) => Promise<void>; native: boolean; loaded: boolean };

const ZOOM: Record<TextSize, string> = { small: "0.92", default: "1", large: "1.08" };

const SettingsContext = createContext<Store | null>(null);

export function applyTextSize(size: TextSize) {
  const root = document.getElementById("root");
  if (root) (root.style as CSSStyleDeclaration & { zoom: string }).zoom = ZOOM[size];
}

const fromServer = (s: ServerSettings): Pick<Settings, "appearance" | "sidebar"> => ({
  appearance: { theme: s.appearance.theme, text_size: s.appearance.text_size },
  sidebar: { open: s.sidebar.open },
});

export function SettingsProvider({ children }: { children: ReactNode }) {
  const native = inTauri();
  const live = useLive();
  const [settings, setSettings] = useState<Settings>(() => ({
    appearance: { theme: "system", text_size: "default" },
    sidebar: { open: true },
    autostart: null,
    notifications: { enabled: null },
  }));
  const [loaded, setLoaded] = useState(false);
  const applied = useRef<Pick<Settings, "appearance"> | null>(null);

  const apply = useCallback((s: Pick<Settings, "appearance">) => {
    if (applied.current?.appearance.theme !== s.appearance.theme) setThemePreference(s.appearance.theme);
    if (applied.current?.appearance.text_size !== s.appearance.text_size) applyTextSize(s.appearance.text_size);
    applied.current = s;
    cacheAppearance(s.appearance);
  }, []);

  const load = useCallback(async () => {
    try {
      const s = fromServer(await api.settings());
      apply(s);
      setSettings((prev) => ({ ...prev, ...s }));
      setLoaded(true);
    } catch {
      // the server is away; the cached appearance stands until it is back
    }
  }, [apply]);

  useEffect(() => {
    load();
  }, [load]);

  // a change made elsewhere — the CLI, the file, another window
  useEffect(() => {
    if (live.lastNotice?.kind === "settings_changed") load();
  }, [live.lastNotice, load]);
  useEffect(() => {
    if (live.connected) load();
  }, [live.connected, load]);

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
      // the shell's own settings go to the core; applied at once, confirmed by the response
      if (patch.appearance || patch.sidebar) {
        const next = {
          appearance: { ...settings.appearance, ...patch.appearance },
          sidebar: { ...settings.sidebar, ...patch.sidebar },
        };
        apply(next);
        setSettings((s) => ({ ...s, ...next }));
        const body: Record<string, unknown> = {};
        if (patch.appearance) body.appearance = patch.appearance;
        if (patch.sidebar) body.sidebar = patch.sidebar;
        try {
          const s = fromServer(await api.patchSettings(body));
          apply(s);
          setSettings((prev) => ({ ...prev, ...s }));
        } catch {
          // refused or the server is away: what was applied stays for this session
        }
      }
      if (native && (patch.autostart !== undefined || patch.notifications?.enabled !== undefined)) {
        const { invoke } = await import("@tauri-apps/api/core");
        if (patch.autostart !== undefined) await invoke("set_autostart", { enabled: patch.autostart });
        if (patch.notifications?.enabled !== undefined) await invoke("set_notifications_paused", { paused: !patch.notifications.enabled });
        setSettings((s) => ({
          ...s,
          autostart: patch.autostart ?? s.autostart,
          notifications: { ...s.notifications, ...patch.notifications },
        }));
      }
    },
    [native, settings.appearance, settings.sidebar, apply],
  );

  const value = useMemo(() => ({ settings, update, native, loaded }), [settings, update, native, loaded]);
  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}

export function useSettings(): Store {
  const store = useContext(SettingsContext);
  if (!store) throw new Error("useSettings outside SettingsProvider");
  return store;
}
