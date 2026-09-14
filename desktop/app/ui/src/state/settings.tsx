// The settings, from the core's settings.json through the API: read at
// start, applied as they arrive, changed with a PATCH, and followed live
// when something else changes them (the CLI, a hand edit, the tray). The
// last appearance is cached in localStorage for the first paint only.
// Launch-at-login is the app's own, through a command.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api, inTauri } from "../api/client";
import type { ServerSettings } from "../api/types";
import { DEFAULT_GLOBAL_SHORTCUT } from "../lib/shortcuts";
import { cacheAppearance, setThemePreference, type ThemePreference } from "../lib/theme";
import { useLive } from "./live";

export type TextSize = "small" | "default" | "large";

export type Settings = {
  appearance: { theme: ThemePreference; text_size: TextSize };
  sidebar: { open: boolean };
  close_window: "hide" | "quit";
  menu_bar_icon: boolean;
  notifications: { enabled: boolean; paused_until: string | null; sound: boolean };
  shortcut: { global: string; global_opens: "oldest" | "inbox" };
  /** launch at login; null when the app cannot say (a browser) */
  autostart: boolean | null;
};

type Patch = {
  appearance?: Partial<Settings["appearance"]>;
  sidebar?: Partial<Settings["sidebar"]>;
  close_window?: Settings["close_window"];
  menu_bar_icon?: boolean;
  notifications?: Partial<Settings["notifications"]>;
  shortcut?: Partial<Settings["shortcut"]>;
  autostart?: boolean;
};

type Store = { settings: Settings; update: (patch: Patch) => Promise<void>; native: boolean; loaded: boolean };

const ZOOM: Record<TextSize, string> = { small: "0.92", default: "1", large: "1.08" };

const SettingsContext = createContext<Store | null>(null);

export function applyTextSize(size: TextSize) {
  const root = document.getElementById("root");
  if (root) (root.style as CSSStyleDeclaration & { zoom: string }).zoom = ZOOM[size];
}

type Served = Pick<Settings, "appearance" | "sidebar" | "close_window" | "menu_bar_icon" | "notifications" | "shortcut">;
const fromServer = (s: ServerSettings): Served => ({
  appearance: { theme: s.appearance.theme, text_size: s.appearance.text_size },
  sidebar: { open: s.sidebar.open },
  close_window: s.close_window,
  menu_bar_icon: s.menu_bar_icon,
  notifications: { enabled: s.notifications.enabled, paused_until: s.notifications.paused_until, sound: s.notifications.sound },
  shortcut: { global: s.shortcut.global, global_opens: s.shortcut.global_opens },
});

export function SettingsProvider({ children }: { children: ReactNode }) {
  const native = inTauri();
  const live = useLive();
  const [settings, setSettings] = useState<Settings>(() => ({
    appearance: { theme: "system", text_size: "default" },
    sidebar: { open: true },
    close_window: "hide",
    menu_bar_icon: true,
    notifications: { enabled: true, paused_until: null, sound: true },
    shortcut: { global: DEFAULT_GLOBAL_SHORTCUT, global_opens: "oldest" },
    autostart: null,
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

  // what the app knows: launch at login
  useEffect(() => {
    if (!native) return;
    let cancelled = false;
    import("@tauri-apps/api/core").then(({ invoke }) =>
      invoke<boolean>("autostart_enabled")
        .then((autostart) => !cancelled && setSettings((s) => ({ ...s, autostart })))
        .catch(() => {}),
    );
    return () => {
      cancelled = true;
    };
  }, [native]);

  const update = useCallback(
    async (patch: Patch) => {
      // the core's settings go to the core; applied at once, confirmed by the response
      if (patch.appearance || patch.sidebar || patch.close_window !== undefined || patch.menu_bar_icon !== undefined || patch.notifications || patch.shortcut) {
        const next: Served = {
          appearance: { ...settings.appearance, ...patch.appearance },
          sidebar: { ...settings.sidebar, ...patch.sidebar },
          close_window: patch.close_window ?? settings.close_window,
          menu_bar_icon: patch.menu_bar_icon ?? settings.menu_bar_icon,
          notifications: { ...settings.notifications, ...patch.notifications },
          shortcut: { ...settings.shortcut, ...patch.shortcut },
        };
        apply(next);
        setSettings((s) => ({ ...s, ...next }));
        const body: Record<string, unknown> = {};
        if (patch.appearance) body.appearance = patch.appearance;
        if (patch.sidebar) body.sidebar = patch.sidebar;
        if (patch.close_window !== undefined) body.close_window = patch.close_window;
        if (patch.menu_bar_icon !== undefined) body.menu_bar_icon = patch.menu_bar_icon;
        if (patch.notifications) body.notifications = patch.notifications;
        if (patch.shortcut) body.shortcut = patch.shortcut;
        try {
          const s = fromServer(await api.patchSettings(body));
          apply(s);
          setSettings((prev) => ({ ...prev, ...s }));
        } catch {
          // refused or the server is away: what was applied stays for this session
        }
      }
      if (native && patch.autostart !== undefined) {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("set_autostart", { enabled: patch.autostart });
        setSettings((s) => ({ ...s, autostart: patch.autostart ?? s.autostart }));
      }
    },
    [native, settings.appearance, settings.sidebar, settings.close_window, settings.menu_bar_icon, settings.notifications, settings.shortcut, apply],
  );

  const value = useMemo(() => ({ settings, update, native, loaded }), [settings, update, native, loaded]);
  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}

export function useSettings(): Store {
  const store = useContext(SettingsContext);
  if (!store) throw new Error("useSettings outside SettingsProvider");
  return store;
}
